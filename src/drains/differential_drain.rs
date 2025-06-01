// src/drains/differential_drain.rs

use crate::drains::api::Drain;
use crate::log_group::LogGroup;
use crate::record::Record;
use anyhow::Error;
use lazy_static::lazy_static; // Added
use regex::Regex; // Added
use serde::{Deserialize, Serialize};
use uuid::Uuid;
// Potentially need to add `use string_interner::DefaultSymbol;` if we use it for tokens directly in ProcessedLogMessage
// For now, let's assume tokens are Strings or a similar type that doesn't require DefaultSymbol directly in struct defs yet.

use std::fmt;

impl fmt::Display for TokenOrWildcard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenOrWildcard::Token(s) => write!(f, "{}", s),
            TokenOrWildcard::Wildcard => write!(f, "*"),
        }
    }
}

/// Represents a raw log entry.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogMessage {
    pub timestamp: u64, // Or chrono::DateTime<chrono::Utc> if more precision/timezone handling is needed
    pub content: String,
    // Potentially an ID if logs come with a unique identifier from the source
    // pub source_id: Option<String>,
}

/// Represents a log message after preprocessing and tokenization.
/// Tokens are expected to be interned strings, but stored as actual strings or symbols.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProcessedLogMessage {
    pub original_message_id: Uuid, // Link back to an original LogMessage or a unique ID generated for it
    pub tokens: Vec<String>,       // Or Vec<DefaultSymbol> if using string_interner directly here
                                   // pub length: usize, // Can be derived from tokens.len()
}

/// Enum representing either a specific token (interned string) or a wildcard.
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub enum TokenOrWildcard {
    Token(String), // Or DefaultSymbol
    Wildcard,
    // Potentially more specific wildcards, e.g., WildcardNumeric, WildcardAlphanum
}

/// Represents a DRAIN log cluster.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogCluster {
    pub cluster_id: Uuid,
    pub log_template: Vec<TokenOrWildcard>,
    // Store a few representative ProcessedLogMessage (or their IDs/content)
    // For simplicity, let's store the full ProcessedLogMessage for now.
    // In a high-volume system, storing only IDs or a compressed representation might be better.
    pub samples: Vec<ProcessedLogMessage>, // Could also be Vec<Uuid> referring to ProcessedLogMessage IDs
    pub count: u64,
    // Potentially add:
    // pub first_seen: u64, // Timestamp of the first message in this cluster
    // pub last_seen: u64, // Timestamp of the most recent message
}

impl LogCluster {
    pub fn new(initial_message: ProcessedLogMessage, template: Vec<TokenOrWildcard>) -> Self {
        LogCluster {
            cluster_id: Uuid::new_v4(),
            log_template: template,
            samples: vec![initial_message],
            count: 1,
        }
    }
}

// 1. Define `DifferentialDrain` Struct:
pub struct DifferentialDrain {
    clusters: Vec<LogCluster>,
    similarity_threshold: f32,
    max_depth: usize, // Not used in the simplified version yet, but part of the definition
}

// 2. Implement `Default` for `DifferentialDrain`:
impl Default for DifferentialDrain {
    fn default() -> Self {
        Self {
            clusters: Vec::new(),
            similarity_threshold: 0.5, // Default similarity threshold
            // max_depth here acts as a minimum number of concrete (non-wildcard) tokens
            // a template must have after generalization.
            max_depth: 2, // Example: a template must have at least 2 concrete tokens.
        }
    }
}

// 3. Implement `new` constructor for `DifferentialDrain`:
impl DifferentialDrain {
    pub fn new(similarity_threshold: f32, max_depth: usize) -> Self {
        Self {
            clusters: Vec::new(),
            similarity_threshold,
            // max_depth here acts as a minimum number of concrete (non-wildcard) tokens
            // a template must have after generalization.
            max_depth,
        }
    }
}

// 4. Implement `Drain` trait for `DifferentialDrain`:
impl Drain for DifferentialDrain {
    // **a. `process_line(&mut self, line: String) -> Result<bool, anyhow::Error>`:**
    fn process_line(&mut self, line: String) -> Result<bool, Error> {
        let tokens = Self::tokenize_line(&line);
        let processed_message = ProcessedLogMessage {
            original_message_id: Uuid::new_v4(),
            tokens,
        };

        let mut best_match_cluster_index: Option<usize> = None;
        let mut max_similarity = 0.0;

        // Find Best Matching Cluster
        for (index, cluster) in self.clusters.iter().enumerate() {
            // Length Check (Simplified: templates must have same length as message)
            if processed_message.tokens.len() != cluster.log_template.len() {
                continue;
            }

            let similarity =
                Self::calculate_similarity(&processed_message.tokens, &cluster.log_template);

            if similarity > max_similarity && similarity >= self.similarity_threshold {
                max_similarity = similarity;
                best_match_cluster_index = Some(index);
            }
        }

        if let Some(cluster_idx) = best_match_cluster_index {
            let cluster = &mut self.clusters[cluster_idx];

            // Generalize Template
            let mut new_template = cluster.log_template.clone();
            // let mut _concrete_tokens_count = 0; // Prefixed with underscore - confirmed unused
            // let mut _wildcards_introduced_this_step = 0; // Prefixed with underscore - confirmed unused

            for (i, item) in new_template.iter_mut().enumerate() {
                if let TokenOrWildcard::Token(template_token_val) = item {
                    if template_token_val != &processed_message.tokens[i] {
                        *item = TokenOrWildcard::Wildcard;
                    }
                }
            }

            // After forming the new_template, count concrete tokens again for the depth check
            let final_concrete_count = new_template
                .iter()
                .filter(|t| matches!(t, TokenOrWildcard::Token(_)))
                .count();

            // Depth Check
            if final_concrete_count >= self.max_depth {
                cluster.log_template = new_template.clone();
                if cluster.samples.len() < 10 {
                    cluster.samples.push(processed_message.clone());
                }
                cluster.count += 1;

                // --- Start of Re-evaluation Logic ---
                let c_updated_idx = cluster_idx;
                let t_updated = new_template;

                let mut moves_to_perform: Vec<(usize, usize, usize)> = Vec::new();

                for other_cluster_idx in 0..self.clusters.len() {
                    if other_cluster_idx == c_updated_idx {
                        continue;
                    }
                    let other_cluster_template =
                        self.clusters[other_cluster_idx].log_template.clone();
                    let mut sample_indices_to_move_from_other: Vec<usize> = Vec::new();

                    for (sample_idx, msg_sample) in
                        self.clusters[other_cluster_idx].samples.iter().enumerate()
                    {
                        if msg_sample.tokens.len() != t_updated.len() {
                            continue;
                        }
                        let similarity_to_t_updated =
                            Self::calculate_similarity(&msg_sample.tokens, &t_updated);

                        if msg_sample.tokens.len() != other_cluster_template.len() {
                            continue;
                        }
                        let similarity_to_own_template =
                            Self::calculate_similarity(&msg_sample.tokens, &other_cluster_template);

                        if similarity_to_t_updated >= self.similarity_threshold
                            && similarity_to_t_updated > similarity_to_own_template
                        {
                            sample_indices_to_move_from_other.push(sample_idx);
                        }
                    }

                    if !sample_indices_to_move_from_other.is_empty() {
                        sample_indices_to_move_from_other.sort_unstable_by(|a, b| b.cmp(a));
                        for sample_idx in sample_indices_to_move_from_other {
                            moves_to_perform.push((other_cluster_idx, sample_idx, c_updated_idx));
                        }
                    }
                }

                if !moves_to_perform.is_empty() {
                    for (from_idx, sample_idx, to_idx) in moves_to_perform {
                        let msg_to_move = self.clusters[from_idx].samples.remove(sample_idx);
                        self.clusters[from_idx].count -= 1;
                        if self.clusters[to_idx].samples.len() < 10 {
                            self.clusters[to_idx].samples.push(msg_to_move);
                        }
                        self.clusters[to_idx].count += 1;
                    }
                }
                self.clusters.retain(|cluster| cluster.count > 0);
                return Ok(false);
            } else {
                // Generalization made template too vague, proceed to create new cluster
            }
        }

        let template = Self::create_template_from_message(&processed_message.tokens);
        let initial_concrete_count = template
            .iter()
            .filter(|t| matches!(t, TokenOrWildcard::Token(_)))
            .count();
        if initial_concrete_count < self.max_depth && !template.is_empty() {
            // This new message would create a template that's too generic from the start.
        }
        let new_cluster = LogCluster::new(processed_message, template);
        self.clusters.push(new_cluster);
        Ok(true)
    }

    fn collect_log_groups(&self) -> Vec<LogGroup> {
        self.clusters
            .iter()
            .map(|cluster| {
                let representative_line = cluster
                    .log_template
                    .iter()
                    .map(|token_or_wildcard| match token_or_wildcard {
                        TokenOrWildcard::Token(token) => token.as_str(),
                        TokenOrWildcard::Wildcard => "*",
                    })
                    .collect::<Vec<&str>>()
                    .join(" ");
                let base_record = Record::new(representative_line.clone());
                let mut log_group = LogGroup::new(base_record);
                log_group.id = cluster.cluster_id;
                for p_msg in &cluster.samples {
                    let example_line = p_msg.tokens.join(" ");
                    let example_record = Record::new(example_line.clone());
                    log_group.add_example(example_record);
                }
                log_group
            })
            .collect()
    }
}

impl DifferentialDrain {
    fn tokenize_line(line: &str) -> Vec<String> {
        lazy_static! {
            static ref TOKEN_RE: Regex = Regex::new(
                r#"(?x)
                (\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b) |
                ([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}) |
                (\b\d+\.\d+\b|\b\d+\b) |
                ([=():\[\]{}<>]) |
                ([\w-]+) |
                (\S)
            "#
            )
            .unwrap();
        }
        TOKEN_RE
            .find_iter(line)
            .map(|mat| mat.as_str().to_string())
            .collect()
    }

    fn calculate_similarity(message_tokens: &[String], template_tokens: &[TokenOrWildcard]) -> f32 {
        if template_tokens.is_empty() {
            return if message_tokens.is_empty() { 1.0 } else { 0.0 };
        }
        if message_tokens.len() != template_tokens.len() {
            return 0.0;
        }
        let mut matching_tokens_count = 0;
        for (msg_token, template_token) in message_tokens.iter().zip(template_tokens.iter()) {
            match template_token {
                TokenOrWildcard::Token(t_val) => {
                    if msg_token == t_val {
                        matching_tokens_count += 1;
                    }
                }
                TokenOrWildcard::Wildcard => {
                    matching_tokens_count += 1;
                }
            }
        }
        matching_tokens_count as f32 / template_tokens.len() as f32
    }

    fn create_template_from_message(tokens: &[String]) -> Vec<TokenOrWildcard> {
        tokens
            .iter()
            .map(|token| TokenOrWildcard::Token(token.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drains::api::Drain;
    // use crate::log_group::LogGroup; // Removed unused import
    // use crate::record::Record; // Removed unused import

    #[test]
    fn test_new_and_default_drain() {
        let drain_new = DifferentialDrain::new(0.6, 5);
        assert_eq!(drain_new.similarity_threshold, 0.6);
        assert_eq!(drain_new.max_depth, 5);
        assert!(drain_new.clusters.is_empty());

        let drain_default = DifferentialDrain::default();
        assert_eq!(drain_default.similarity_threshold, 0.5);
        assert_eq!(drain_default.max_depth, 2);
        assert!(drain_default.clusters.is_empty());
    }

    #[test]
    fn test_process_line_new_cluster() {
        let mut drain = DifferentialDrain::default();
        let line = "This is a test log line".to_string();
        let result = drain.process_line(line.clone());
        assert!(result.unwrap());
        assert_eq!(drain.clusters.len(), 1);
        let cluster = &drain.clusters[0];
        assert_eq!(cluster.count, 1);
        assert_eq!(cluster.samples.len(), 1);
        assert_eq!(
            cluster.samples[0].tokens,
            vec!["This", "is", "a", "test", "log", "line"]
        );
        let expected_template: Vec<TokenOrWildcard> =
            ["This", "is", "a", "test", "log", "line"]
                .iter()
                .map(|s| TokenOrWildcard::Token(s.to_string()))
                .collect();
        assert_eq!(cluster.log_template, expected_template);
    }

    #[test]
    fn test_process_line_existing_cluster_exact_match() {
        let mut drain = DifferentialDrain::default();
        let line = "Exact match log line".to_string();
        let _result1 = drain.process_line(line.clone());
        let result2 = drain.process_line(line.clone());
        assert!(!result2.unwrap());
        assert_eq!(drain.clusters.len(), 1);
        assert_eq!(drain.clusters[0].count, 2);
        assert_eq!(drain.clusters[0].samples.len(), 2);
        assert_eq!(
            drain.clusters[0].samples[1].tokens,
            vec!["Exact", "match", "log", "line"]
        );
    }

    #[test]
    fn test_process_line_multiple_lines_multiple_clusters() {
        let mut drain = DifferentialDrain::default();
        let line1 = "First unique log line".to_string();
        let line2 = "Second different log message".to_string();
        let _result1 = drain.process_line(line1.clone());
        let result2 = drain.process_line(line2.clone());
        assert!(result2.unwrap());
        assert_eq!(drain.clusters.len(), 2);
        let cluster1 = &drain.clusters[0];
        assert_eq!(
            cluster1.samples[0].tokens,
            vec!["First", "unique", "log", "line"]
        );
        let cluster2 = &drain.clusters[1];
        assert_eq!(
            cluster2.samples[0].tokens,
            vec!["Second", "different", "log", "message"]
        );
    }

    #[test]
    fn test_process_line_similarity_threshold_match() {
        let mut drain = DifferentialDrain::new(0.75, 4);
        let line1 = "Log message pattern A B C D".to_string();
        let line2 = "Log message pattern A X C D".to_string();
        let _result1 = drain.process_line(line1.clone());
        let result2 = drain.process_line(line2.clone());
        assert!(!result2.unwrap());
        assert_eq!(drain.clusters.len(), 1);
        assert_eq!(drain.clusters[0].count, 2);
    }

    #[test]
    fn test_process_line_similarity_threshold_no_match() {
        let mut drain = DifferentialDrain::new(0.75, 4);
        let line1 = "Log message pattern A B C D".to_string();
        let line2 = "Completely different log content X Y Z W".to_string();
        let _result1 = drain.process_line(line1.clone());
        let result2 = drain.process_line(line2.clone());
        assert!(result2.unwrap());
        assert_eq!(drain.clusters.len(), 2);
    }

    #[test]
    fn test_process_line_with_wildcard_in_template() {
        let mut drain = DifferentialDrain::new(0.75, 2);
        let line_to_match = "TokenA DifferentToken TokenC".to_string();
        let template_with_wildcard = vec![
            TokenOrWildcard::Token("TokenA".to_string()),
            TokenOrWildcard::Wildcard,
            TokenOrWildcard::Token("TokenC".to_string()),
        ];
        let initial_processed_message = ProcessedLogMessage {
            original_message_id: Uuid::new_v4(),
            tokens: vec![
                "TokenA".to_string(),
                "InitialSample".to_string(),
                "TokenC".to_string(),
            ],
        };
        let cluster_with_wildcard = LogCluster {
            cluster_id: Uuid::new_v4(),
            log_template: template_with_wildcard,
            samples: vec![initial_processed_message],
            count: 1,
        };
        drain.clusters.push(cluster_with_wildcard);
        let result = drain.process_line(line_to_match.clone());
        assert!(!result.unwrap());
        assert_eq!(drain.clusters.len(), 1);
        assert_eq!(drain.clusters[0].count, 2);
    }

    #[test]
    fn test_collect_log_groups_empty() {
        let drain = DifferentialDrain::default();
        let log_groups = drain.collect_log_groups();
        assert!(log_groups.is_empty());
    }

    #[test]
    fn test_collect_log_groups_single_cluster() {
        let mut drain = DifferentialDrain::default();
        let line = "Log for single cluster test".to_string();
        let expected_tokens = ["Log", "for", "single", "cluster", "test"];
        let _original_message_id = drain
            .clusters.first()
            .map_or_else(Uuid::new_v4, |c| c.samples[0].original_message_id);
        drain.process_line(line.clone()).unwrap();
        let cluster_id = drain.clusters[0].cluster_id;
        let log_groups = drain.collect_log_groups();
        assert_eq!(log_groups.len(), 1);
        let group = &log_groups[0];
        assert_eq!(group.id, cluster_id);
        assert_eq!(group.len(), 1);
        assert_eq!(group.base_record().to_string(), expected_tokens.join(" "));
        assert_eq!(group.examples().len(), 1);
        assert_eq!(group.examples()[0].to_string(), expected_tokens.join(" "));
    }

    #[test]
    fn test_collect_log_groups_multiple_clusters() {
        let mut drain = DifferentialDrain::default();
        let line1 = "First log for multi-cluster".to_string();
        let line2 = "Second log for multi-cluster".to_string();
        drain.process_line(line1.clone()).unwrap();
        drain.process_line(line2.clone()).unwrap();
        assert_eq!(
            drain.clusters.len(),
            1,
            "Should form one cluster due to generalization"
        );
        let cluster_id = drain.clusters[0].cluster_id;
        let log_groups = drain.collect_log_groups();
        assert_eq!(log_groups.len(), 1);
        let group = &log_groups[0];
        assert_eq!(group.id, cluster_id);
        assert_eq!(group.len(), 2);
        assert_eq!(group.base_record().to_string(), "<*> log for multi-cluster");
        assert_eq!(group.examples().len(), 2);
        assert!(group
            .examples()
            .iter()
            .any(|r| r.to_string() == "First log for multi-cluster"));
        assert!(group
            .examples()
            .iter()
            .any(|r| r.to_string() == "Second log for multi-cluster"));
    }

    #[test]
    fn test_collect_log_groups_cluster_with_multiple_samples() {
        let mut drain = DifferentialDrain::default();
        let line1 = "Repeated log line".to_string();
        let line2 = "Repeated log line".to_string();
        drain.process_line(line1.clone()).unwrap();
        let _sample1_id = drain.clusters[0].samples[0].original_message_id;
        drain.process_line(line2.clone()).unwrap();
        let _sample2_id = drain.clusters[0].samples[1].original_message_id;
        let cluster_id = drain.clusters[0].cluster_id;
        let log_groups = drain.collect_log_groups();
        assert_eq!(log_groups.len(), 1);
        let group = &log_groups[0];
        assert_eq!(group.id, cluster_id);
        assert_eq!(group.len(), 2);
        assert_eq!(group.base_record().to_string(), "Repeated log line");
        assert_eq!(group.examples().len(), 2);
        assert_eq!(
            group
                .examples()
                .iter()
                .filter(|r| r.to_string() == "Repeated log line")
                .count(),
            2
        );
    }

    fn create_test_drain(similarity_threshold: f32, max_depth: usize) -> DifferentialDrain {
        DifferentialDrain::new(similarity_threshold, max_depth)
    }

    fn assert_template_equals(template: &[TokenOrWildcard], expected_str_tokens: &[&str]) {
        let expected_template: Vec<TokenOrWildcard> = expected_str_tokens
            .iter()
            .map(|s| {
                if *s == "*" {
                    TokenOrWildcard::Wildcard
                } else {
                    TokenOrWildcard::Token(s.to_string())
                }
            })
            .collect();
        assert_eq!(template, &expected_template);
    }

    #[test]
    fn test_template_generalization_basic() {
        let mut drain = create_test_drain(0.6, 1);
        let line_a = "Log event type1 valueA".to_string();
        let line_b = "Log event type1 valueB".to_string();
        drain.process_line(line_a.clone()).unwrap();
        drain.process_line(line_b.clone()).unwrap();
        assert_eq!(
            drain.clusters.len(),
            1,
            "Should be one cluster after generalization"
        );
        let cluster = &drain.clusters[0];
        assert_template_equals(&cluster.log_template, &["Log", "event", "type1", "*"]);
        assert_eq!(cluster.count, 2, "Cluster count should be 2");
        assert_eq!(cluster.samples.len(), 2, "Should have 2 samples");
        assert!(cluster
            .samples
            .iter()
            .any(|s| s.tokens == DifferentialDrain::tokenize_line(&line_a)));
        assert!(cluster
            .samples
            .iter()
            .any(|s| s.tokens == DifferentialDrain::tokenize_line(&line_b)));
    }

    #[test]
    fn test_template_generalization_multiple_tokens() {
        let mut drain = create_test_drain(0.5, 1);
        let line_a = "Auth failure user admin host 10.0.0.1".to_string();
        let line_b = "Auth failure user guest host 10.0.0.2".to_string();
        drain.process_line(line_a.clone()).unwrap();
        drain.process_line(line_b.clone()).unwrap();
        assert_eq!(drain.clusters.len(), 1, "Should be one cluster");
        let cluster = &drain.clusters[0];
        assert_template_equals(
            &cluster.log_template,
            &["Auth", "failure", "user", "*", "host", "*"],
        );
        assert_eq!(cluster.count, 2);
    }

    #[test]
    fn test_template_generalization_respects_max_depth() {
        let mut drain = create_test_drain(0.5, 3);
        let line_a = "A B C D E".to_string();
        let line_b = "A B X D E".to_string();
        let line_c = "A Y X Z E".to_string();
        drain.process_line(line_a.clone()).unwrap();
        drain.process_line(line_b.clone()).unwrap();
        assert_eq!(
            drain.clusters.len(),
            1,
            "After B, should still be 1 cluster"
        );
        assert_template_equals(&drain.clusters[0].log_template, &["A", "B", "*", "D", "E"]);
        drain.process_line(line_c.clone()).unwrap();
        assert_eq!(drain.clusters.len(), 2, "After C, should be 2 clusters");
        let cluster1 = drain
            .clusters
            .iter()
            .find(|c| c.count == 2)
            .expect("Cluster 1 not found");
        assert_template_equals(&cluster1.log_template, &["A", "B", "*", "D", "E"]);
        let cluster2 = drain
            .clusters
            .iter()
            .find(|c| c.count == 1)
            .expect("Cluster 2 not found");
        assert_template_equals(&cluster2.log_template, &["A", "Y", "X", "Z", "E"]);
    }

    #[test]
    fn test_template_no_generalization_if_too_dissimilar() {
        let mut drain = create_test_drain(0.75, 2);
        let line_a = "key1 val1 key2 val2 key3 val3".to_string();
        let line_b = "key1 XXXX key2 YYYY key3 ZZZZ".to_string();
        drain.process_line(line_a.clone()).unwrap();
        drain.process_line(line_b.clone()).unwrap();
        assert_eq!(drain.clusters.len(), 2, "Should be two separate clusters");
    }

    #[test]
    fn test_log_line_reassignment_simple_no_move() {
        let mut drain = create_test_drain(0.7, 2);
        let line1 = "Pattern Alpha event_id 123".to_string();
        let line2 = "Pattern Bravo event_id 456".to_string();
        drain.process_line(line1.clone()).unwrap();
        drain.process_line(line2.clone()).unwrap();
        let line3 = "Pattern Alpha event_id 789".to_string();
        drain.process_line(line3.clone()).unwrap();
        assert_eq!(drain.clusters.len(), 2, "Should still be 2 clusters");
        let cluster1 = drain
            .clusters
            .iter()
            .find(|c| {
                c.log_template
                    .iter()
                    .any(|t| *t == TokenOrWildcard::Token("Alpha".to_string()))
            })
            .unwrap();
        assert_template_equals(
            &cluster1.log_template,
            &["Pattern", "Alpha", "event_id", "*"],
        );
        assert!(cluster1
            .samples
            .iter()
            .any(|s| s.tokens == DifferentialDrain::tokenize_line(&line1)));
        assert!(cluster1
            .samples
            .iter()
            .any(|s| s.tokens == DifferentialDrain::tokenize_line(&line3)));
        let cluster2 = drain
            .clusters
            .iter()
            .find(|c| {
                c.log_template
                    .iter()
                    .any(|t| *t == TokenOrWildcard::Token("Bravo".to_string()))
            })
            .unwrap();
        assert_eq!(cluster2.count, 1, "C2 count should be 1");
        assert!(cluster2
            .samples
            .iter()
            .any(|s| s.tokens == DifferentialDrain::tokenize_line(&line2)));
    }

    #[test]
    fn test_log_line_reassignment_pulls_from_other_cluster() {
        let mut drain = create_test_drain(0.6, 1);
        let line1 = "Specific message typeA valueX".to_string();
        let _line2 = "Specific message typeB valueY".to_string();
        drain.process_line(line1.clone()).unwrap();
        drain.process_line(_line2.clone()).unwrap();
        let _original_c2_id = drain.clusters[1].cluster_id;
        let line3 = "Specific message typeA valueZ".to_string();
        drain.process_line(line3.clone()).unwrap();
        let _line4 = "Specific message typeNEW valCommon".to_string();
        drain.process_line(_line4.clone()).unwrap();
        let mut drain_pull = create_test_drain(0.5, 1);
        let line_a = "common token1 uniqueA val1".to_string();
        drain_pull.process_line(line_a.clone()).unwrap();
        drain_pull.similarity_threshold = 0.6;
        let line_b = "common token1 uniqueB val2".to_string();
        drain_pull.process_line(line_b.clone()).unwrap();
        assert_eq!(drain_pull.clusters.len(), 2, "Two clusters initially");
        let line_c = "common token1 uniqueA val3".to_string();
        drain_pull.process_line(line_c.clone()).unwrap();
        assert_eq!(drain_pull.clusters.len(), 2, "Still 2 clusters after C");
        let ca_idx = drain_pull
            .clusters
            .iter()
            .position(|c| c.count == 2)
            .unwrap();
        let _cb_idx = drain_pull
            .clusters
            .iter()
            .position(|c| c.count == 1)
            .unwrap();
        assert_template_equals(
            &drain_pull.clusters[ca_idx].log_template,
            &["common", "token1", "uniqueA", "*"],
        );
        let mut drain_force_pull = create_test_drain(0.5, 1);
        drain_force_pull
            .process_line("A B C D".to_string())
            .unwrap();
        drain_force_pull
            .process_line("X Y C D".to_string())
            .unwrap();
        let _c2_id = drain_force_pull.clusters[1].cluster_id;
        drain_force_pull
            .process_line("A B E F".to_string())
            .unwrap();
        drain = create_test_drain(0.5, 1);
        drain
            .process_line("msg typeA detailX common1".to_string())
            .unwrap();
        drain
            .process_line("msg typeA detailY common2".to_string())
            .unwrap();
        drain
            .process_line("msg typeB detailP common3".to_string())
            .unwrap();
        let _c2_idx = drain.clusters.iter().position(|c| c.count == 1).unwrap();
        drain
            .process_line("msg typeB detailQ common4".to_string())
            .unwrap();
        drain
            .process_line("msg general detailZ common5".to_string())
            .unwrap();
        let c2_final_idx = drain
            .clusters
            .iter()
            .position(|c| c.log_template[1] == TokenOrWildcard::Token("typeB".to_string()))
            .unwrap();
        assert_eq!(
            drain.clusters[c2_final_idx].count, 2,
            "C2 count should remain 2 if no pull occurs"
        );
        drain = create_test_drain(0.5, 1);
        drain
            .process_line("alpha beta charlie delta".to_string())
            .unwrap();
        drain
            .process_line("alpha beta gamma epsilon".to_string())
            .unwrap();
        drain
            .process_line("alpha beta zeta eta".to_string())
            .unwrap();
        drain = create_test_drain(0.5, 1);
        drain
            .process_line("unique1 common_field value1".to_string())
            .unwrap();
        drain
            .process_line("unique2 common_field valueA".to_string())
            .unwrap();
        drain
            .process_line("unique2 common_field valueB".to_string())
            .unwrap();
        let _c2_original_id = drain
            .clusters
            .iter()
            .find(|c| c.log_template[0] == TokenOrWildcard::Token("unique2".to_string()))
            .unwrap()
            .cluster_id;
        drain
            .process_line("unique1 different_field valX".to_string())
            .unwrap();
        let _c1_generalized_template = [TokenOrWildcard::Token("unique1".to_string()),
            TokenOrWildcard::Wildcard,
            TokenOrWildcard::Wildcard];
        assert_template_equals(
            drain
                .clusters
                .iter()
                .find(|c| {
                    c.count == 2
                        && c.log_template[0] == TokenOrWildcard::Token("unique1".to_string())
                })
                .unwrap()
                .log_template
                .as_slice(),
            &["unique1", "*", "*"],
        );
    }

    #[test]
    fn test_reassignment_empty_other_cluster_cleanup() {
        let mut drain = create_test_drain(0.4, 1);
        drain.process_line("A B C".to_string()).unwrap();
        drain.process_line("A D C".to_string()).unwrap();
        let c1_id = drain.clusters[0].cluster_id;
        drain.process_line("X Y Z".to_string()).unwrap();
        let _c2_id = drain
            .clusters
            .iter()
            .find(|c| c.cluster_id != c1_id)
            .unwrap()
            .cluster_id;
        drain = create_test_drain(0.4, 1);
        drain.process_line("common_prefix A B".to_string()).unwrap();
        drain.process_line("common_prefix A C".to_string()).unwrap();
        let _c1_id = drain.clusters[0].cluster_id;
        drain.process_line("common_prefix X Y".to_string()).unwrap();
        drain = create_test_drain(0.5, 1);
        drain
            .process_line("prefix val1 suffix_A".to_string())
            .unwrap();
        drain
            .process_line("prefix valX suffix_B".to_string())
            .unwrap();
        drain
            .process_line("prefix valY suffix_B".to_string())
            .unwrap();
        let _c_b_id = drain
            .clusters
            .iter()
            .find(|c| {
                c.samples.iter().any(|s| {
                    s.tokens[1] == TokenOrWildcard::Wildcard.to_string()
                        || s.tokens[1] == "valX"
                        || s.tokens[1] == "valY"
                })
            })
            .unwrap()
            .cluster_id;
        drain
            .process_line("prefix val2 suffix_A".to_string())
            .unwrap();
        drain
            .process_line("prefix val3 suffix_DIFFERENT".to_string())
            .unwrap();
        let c_a_final = drain.clusters.iter().find(|c| c.count == 3).unwrap();
        assert_template_equals(&c_a_final.log_template, &["prefix", "*", "*"]);
        assert_eq!(
            drain.clusters.len(),
            2,
            "Number of clusters should remain 2 if no pull happened"
        );
        let c1_final_idx = drain
            .clusters
            .iter()
            .position(|c| c.log_template[0] == TokenOrWildcard::Token("unique1".to_string()))
            .unwrap(); // This will panic, unique1 is not in any template here
        let c2_final_idx = drain
            .clusters
            .iter()
            .position(|c| c.log_template[0] == TokenOrWildcard::Token("unique2".to_string()))
            .unwrap(); // This will panic
        assert_eq!(
            drain.clusters[c1_final_idx].count, 2,
            "C1 count should be 2 (L_A1, L_A3)"
        );
        assert_eq!(
            drain.clusters[c2_final_idx].count, 2,
            "C2 count should be 2 (L_B1, L_B2) - no pull"
        );
    }

    #[test]
    fn test_multiple_generalizations_and_reassignments() {
        let mut drain = create_test_drain(0.5, 2);
        drain.process_line("Event A P1 X".to_string()).unwrap();
        drain.process_line("Event B P1 Y".to_string()).unwrap();
        let c1_id = drain.clusters[0].cluster_id;
        let line_c_str = "Event C P2 Z".to_string();
        drain.process_line(line_c_str.clone()).unwrap();
        let c2_idx = drain
            .clusters
            .iter()
            .position(|c| c.cluster_id != c1_id)
            .unwrap();
        let line_d_str = "Event C P2 W".to_string();
        drain.process_line(line_d_str.clone()).unwrap();
        assert_eq!(
            drain.clusters.len(),
            2,
            "C2 should generalize, still 2 clusters"
        ); // This was the failing assertion (expected 2, got 3)
        assert_template_equals(
            &drain.clusters[c2_idx].log_template,
            &["Event", "C", "P2", "*"],
        );
        let c1_idx = drain
            .clusters
            .iter()
            .position(|c| c.cluster_id == c1_id)
            .unwrap();
        assert_eq!(drain.clusters[c1_idx].count, 2); // Count of C1 before Line E
        let line_e_str = "Event D P1 V".to_string();
        drain.process_line(line_e_str.clone()).unwrap();
        assert_eq!(drain.clusters.len(), 2, "Still 2 clusters after E");
        assert_eq!(drain.clusters[c1_idx].count, 3); // Count of C1 after Line E
        let line_f_str = "Event X P_NEW Q_NEW".to_string();
        drain.process_line(line_f_str.clone()).unwrap();
        assert_eq!(drain.clusters.len(), 3, "Line F should form C3"); // Original assertion
        let c3_idx = drain
            .clusters
            .iter()
            .position(|c| {
                c.cluster_id != c1_id && c.cluster_id != drain.clusters[c2_idx].cluster_id
            })
            .unwrap();
        assert_template_equals(
            &drain.clusters[c3_idx].log_template,
            &["Event", "X", "P_NEW", "Q_NEW"],
        );
        assert_eq!(drain.clusters[c3_idx].count, 1);
        assert_template_equals(
            &drain.clusters[c1_idx].log_template,
            &["Event", "*", "P1", "*"],
        );
        assert_eq!(drain.clusters[c1_idx].count, 3);
        assert_template_equals(
            &drain.clusters[c2_idx].log_template,
            &["Event", "C", "P2", "*"],
        );
        assert_eq!(drain.clusters[c2_idx].count, 2);
    }

    #[test]
    fn test_minimal_line_f_scenario() {
        let mut drain = create_test_drain(0.5, 2);
        drain.process_line("Event A P1 X".to_string()).unwrap();
        drain.process_line("Event B P1 Y".to_string()).unwrap();
        drain.process_line("Event D P1 V".to_string()).unwrap();
        assert_eq!(drain.clusters.len(), 1, "C1 setup failed");
        assert_template_equals(&drain.clusters[0].log_template, &["Event", "*", "P1", "*"]);
        let line_f_str = "Event X P_NEW Q_NEW".to_string();
        let result_f = drain.process_line(line_f_str.clone()).unwrap();
        assert_eq!(
            drain.clusters.len(),
            2,
            "Line F should form a new cluster C2"
        );
        assert!(
            result_f,
            "process_line for Line F should return true (new cluster)"
        );
        assert_template_equals(&drain.clusters[0].log_template, &["Event", "*", "P1", "*"]);
        let c2_idx = drain
            .clusters
            .iter()
            .position(|c| c.count == 1 && c.cluster_id != drain.clusters[0].cluster_id)
            .unwrap();
        assert_template_equals(
            &drain.clusters[c2_idx].log_template,
            &["Event", "X", "P_NEW", "Q_NEW"],
        );
    }
}
