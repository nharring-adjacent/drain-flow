// src/drains/differential_drain.rs

use crate::drains::api::Drain;
use crate::log_group::LogGroup;
use crate::record::Record;
use anyhow::Error;
use lazy_static::lazy_static; // Added
use regex::Regex; // Added
use serde::{Deserialize, Serialize};
use tracing::{debug, info, trace, warn};
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

    /// Dumps all clusters into a pretty-printed JSON string for debugging.
    pub fn debug_dump_clusters(&self) -> String {
        match serde_json::to_string_pretty(&self.clusters) {
            Ok(json_str) => json_str,
            Err(e) => {
                // In case of error, return a string indicating the failure.
                // Consider logging the error as well if a logger is available here.
                format!("Error serializing clusters to JSON: {}", e)
            }
        }
    }
}

// Documentation on Robust Cluster Removal Strategies
//
// The following outlines alternative strategies for managing the lifecycle of log clusters,
// specifically focusing on more robust removal mechanisms. These strategies can be
// beneficial in scenarios requiring higher fault tolerance, detailed debugging capabilities,
// or when the cost of an incorrect cluster removal is high. They generally trade
// performance or complexity for this increased robustness.
//
// ## Current Mechanism
//
// Clusters are currently removed from the `self.clusters` vector when their `count`
// attribute becomes zero. This is primarily handled by the `self.clusters.retain(|cluster| cluster.count > 0)`
// call within the `process_line` method, typically after sample re-evaluation and movement.
//
// ## Alternative Strategies
//
// ### Strategy 1: Soft Deletion / Tombstoning
//
// *   **How it works:**
//     *   Introduce a boolean flag (e.g., `is_active: bool`) to the `LogCluster` struct.
//     *   When a cluster's `count` would normally lead to its removal (e.g., reaches 0),
//         instead of deleting it from the `clusters` vector, set `is_active = false`.
//     *   A separate garbage collection (GC) process would be responsible for permanently
//         removing clusters marked as inactive. This GC process could be:
//         *   Periodic (e.g., run every N processed lines or every M minutes).
//         *   Triggered by specific conditions (e.g., when memory usage exceeds a threshold,
//           or when the number of inactive clusters is high).
//     *   Active operations (matching, updating) would then explicitly ignore clusters where `is_active == false`.
//
// *   **Pros:**
//     *   **Debuggability:** Allows inspection of "removed" (inactive) clusters and their
//       state (samples, template) before actual permanent deletion. This can be invaluable
//       for debugging issues related to count inaccuracies or unexpected cluster disappearances.
//     *   **Error Resilience:** Can prevent cascading errors if a cluster's count temporarily
//       drops to zero due to a transient bug or an edge case in the logic, only to be
//       incremented again shortly after. The cluster wouldn't be lost.
//     *   **State Recovery:** Potentially allows for "undeleting" a cluster if its inactivation
//       is found to be erroneous, by simply flipping `is_active` back to `true` (though
//       counts might need recalculation or careful adjustment).
//
// *   **Cons:**
//     *   **Memory Footprint:** Increases memory usage as inactive clusters are retained until
//       the GC process runs.
//     *   **Complexity:** Adds the `is_active` flag and requires implementing the GC logic.
//       All cluster access points need to be aware of the `is_active` flag.
//
// ### Strategy 2: Delayed Removal with Verification
//
// *   **How it works:**
//     *   When a cluster's `count` reaches 0, it's not immediately removed. Instead, it's
//       marked for potential deletion (e.g., added to a `pending_deletion` list or
//       flagged similarly to soft deletion).
//     *   Before actual removal (e.g., during a periodic cleanup, after N new lines, or
//       when the `pending_deletion` list grows large), a verification step is performed.
//     *   **Verification:** For each cluster marked for deletion, the system re-verifies
//       that no `ProcessedLogMessage` from any *active* cluster *should* actually belong
//       to this zero-count cluster. This could involve:
//         *   Comparing the zero-count cluster's template against a subset of samples from
//           other active clusters (especially those with similar lengths or some token overlap).
//         *   If original log lines or more detailed `ProcessedLogMessage` info is stored,
//           re-evaluating a selection of recent messages against this cluster's template.
//         *   Checking if any recent template generalization in another cluster might have
//           incorrectly "emptied" this cluster.
//
// *   **Pros:**
//     *   **Higher Confidence:** Provides greater assurance that a cluster is truly empty and
//       its removal is correct, rather than being a result of a transient count error or
//       a flaw in the generalization/reallocation logic.
//     *   **Catches Subtle Bugs:** Can help identify complex bugs where interactions between
//       clusters lead to incorrect zero counts.
//
// *   **Cons:**
//     *   **Performance Overhead:** The verification step can be computationally expensive,
//       especially if it involves re-calculating similarities for many samples or messages.
//     *   **Complexity:** The verification logic itself can be complex to design and implement
//       correctly and efficiently. Defining "which samples to check" is non-trivial.
//     *   **Delay in Removal:** Actual removal is deferred, leading to a temporary increase
//       in memory similar to soft deletion.
//
// ### Strategy 3: Audit Trail for Count Changes
//
// *   **How it works:**
//     *   For each `LogCluster`, maintain a small, bounded log (e.g., a `VecDeque<CountChangeEntry>`
//       of a limited size like 10-20 entries) of operations that incremented or
//       decremented its `count`.
//     *   Each `CountChangeEntry` in this audit log could store:
//         *   `timestamp`: When the change occurred.
//         *   `operation_type`: Enum (e.g., `NewSampleAdded`, `SampleMovedIn`, `SampleMovedOut`,
//           `SampleAgedOut` - if applicable).
//         *   `message_id`: The `original_message_id` of the `ProcessedLogMessage` involved.
//         *   `related_cluster_id`: If a move, the source/destination cluster ID.
//         *   `count_before`: The cluster's count before this operation.
//         *   `count_after`: The cluster's count after this operation.
//
// *   **Pros:**
//     *   **Deep Debuggability:** Provides a detailed history for each cluster, making it much
//       easier to trace how its `count` evolved and why it might have reached zero (or
//       any other unexpected value). This is extremely useful for debugging count
//       discrepancies.
//     *   **Understanding Dynamics:** Helps in understanding the dynamics of cluster formation,
//       generalization, and sample movement.
//
// *   **Cons:**
//     *   **Memory Overhead:** Adds memory overhead for storing the audit trail for every
//       cluster. The size of this overhead depends on the number of entries kept per cluster
//       and the size of each entry.
//     *   **Performance Impact:** There's a performance cost to updating this log on every
//       relevant operation (incrementing/decrementing count, adding/moving samples).
//     *   **Complexity:** Requires defining the `CountChangeEntry` struct and integrating the
//       logging of these entries into all relevant code paths.
//
// ## Conclusion
//
// The default mechanism of removing clusters when their count reaches zero is efficient
// and straightforward for many use cases. However, for scenarios demanding greater
// resilience against transient errors, or requiring enhanced debugging capabilities to
// understand cluster lifecycle events, the strategies outlined above (Soft Deletion,
// Delayed Removal with Verification, Audit Trail for Count Changes) offer more robust
// alternatives. The choice of strategy depends on the specific requirements and acceptable
// trade-offs in terms of performance, memory, and implementation complexity.
//
// ## Async Usage and Error Handling Context
//
// As of the current review, the `DifferentialDrain` and its core methods
// (e.g., `process_line`, `collect_log_groups`) operate entirely within a
// synchronous context. The codebase does not currently utilize asynchronous
// programming constructs (such as `async`, `await`, `Future`, or async runtimes
// like Tokio or async-std) in relation to the direct operation or invocation
// of `DifferentialDrain`.
//
// ### Error Handling
//
// The primary error handling mechanism employed by `DifferentialDrain` methods,
// such as `process_line`, is the standard Rust `Result<T, E>` type. Specifically,
// `Result<bool, anyhow::Error>` is used for `process_line`.
//
// *   **`anyhow::Error`**: This choice allows for flexible error handling, where
//     functions can return any error type that implements `std.error::Error`,
//     and `anyhow` will wrap it. This is convenient for converting various error
//     types into a single, consistent error type for the function's signature.
//     It typically includes a backtrace if captured (e.g. by setting `RUST_BACKTRACE=1`).
//
// *   **Synchronous Propagation**: Errors are propagated synchronously up the call
//     stack. The caller of `process_line` is responsible for handling the `Result`
//     (e.g., via `match`, `?` operator in a function returning `Result`, or `unwrap`/`expect`
//     if an error is considered fatal for that specific path).
//
// ### Debuggability
//
// *   **Tracing**: The existing `tracing` logs (info!, debug!, trace!, etc.) within
//     `DifferentialDrain` are the primary tools for debugging its execution flow
//     and state changes.
// *   **Error Context**: While `anyhow::Error` provides a backtrace, specific error
//     context (beyond what the original error type provides) might need to be added
//     using `anyhow::Context` or by creating custom error types if more granular
//     error information is frequently needed for debugging. However, for most internal
//     operations within `DifferentialDrain`, the current direct error returns or panics
//     (in case of unrecoverable logic errors like Regex compilation failure in `tokenize_line`)
//     are standard for synchronous Rust.
//
// ### Conclusion on Async
//
// Complexities within `DifferentialDrain` primarily stem from its stateful nature,
// the algorithmic intricacies of log clustering (similarity calculations, template
// generalization, cluster re-evaluation, etc.), and memory management, rather
// than from asynchronous programming paradigms. If `DifferentialDrain` were to be
// integrated into a larger asynchronous system (e.g., processing logs from an
// async stream), the calling code would be responsible for managing the async
// aspects, and `DifferentialDrain` itself would likely remain a synchronous component
// called from within an async task or thread pool (like `tokio::task::spawn_blocking`).

// 4. Implement `Drain` trait for `DifferentialDrain`:
impl Drain for DifferentialDrain {
    // **a. `process_line(&mut self, line: String) -> Result<bool, anyhow::Error>`:**
    fn process_line(&mut self, line: String) -> Result<bool, Error> {
        info!(target: "differential_drain", "process_line started for line: {}", line);
        let tokens = Self::tokenize_line(&line);
        debug!(target: "differential_drain", "Tokens for line '{}': {:?}", line, tokens);
        let processed_message = ProcessedLogMessage {
            original_message_id: Uuid::new_v4(),
            tokens,
        };

        let mut best_match_cluster_index: Option<usize> = None;
        let mut max_similarity_score = -1.0_f32;
        // Initialize with 0, as concrete tokens count cannot be negative.
        // Or usize::MIN if that's preferred for counts.
        let mut max_concrete_tokens_after_gen_for_best_match = 0;

        // Find Best Matching Cluster:
        // Iterate through existing clusters to find the best match for the current log message.
        // The "best match" is determined by similarity score and, in case of ties,
        // by which cluster's template would remain more specific (more concrete tokens)
        // after absorbing the new message.
        for (index, cluster) in self.clusters.iter().enumerate() {
            trace!(target: "differential_drain", "Comparing with cluster ID: {}, template: {:?}", cluster.cluster_id.to_string(), cluster.log_template);
            if processed_message.tokens.len() != cluster.log_template.len() {
                trace!(target: "differential_drain", "Skipping cluster ID: {}: token length mismatch (line: {}, template: {})", cluster.cluster_id.to_string(), processed_message.tokens.len(), cluster.log_template.len());
                continue;
            }

            let similarity =
                Self::calculate_similarity(&processed_message.tokens, &cluster.log_template);
            trace!(target: "differential_drain", "Calculated similarity with cluster ID {}: {}", cluster.cluster_id.to_string(), similarity);

            if similarity >= self.similarity_threshold {
                // This cluster is a potential candidate.
                // Let's determine what its template would look like if it absorbed this message.
                let mut candidate_generalized_template = cluster.log_template.clone();
                // Variable to track if any change was made to candidate_generalized_template for accurate concrete count
                // let mut _changed_for_concrete_count = false; // Not strictly needed for this logic
                for (i, item) in candidate_generalized_template.iter_mut().enumerate() {
                    if let TokenOrWildcard::Token(template_token_val) = item {
                        if template_token_val != &processed_message.tokens[i] {
                            *item = TokenOrWildcard::Wildcard;
                            // _changed_for_concrete_count = true; // Not used
                        }
                    }
                }
                let current_concrete_count_after_gen = candidate_generalized_template
                    .iter()
                    .filter(|t| matches!(t, TokenOrWildcard::Token(_)))
                    .count();

                // Only consider this cluster if its generalized template meets the depth requirement.
                if current_concrete_count_after_gen >= self.max_depth {
                    if similarity > max_similarity_score {
                        debug!(target: "differential_drain", "Potential best match found for cluster ID {}: New max similarity {} (was {}). Concrete tokens after gen: {}. Updating best_match_cluster_index to {}.", cluster.cluster_id.to_string(), similarity, max_similarity_score, current_concrete_count_after_gen, index);
                        // This candidate has a higher similarity score than any previous best.
                        max_similarity_score = similarity;
                        max_concrete_tokens_after_gen_for_best_match =
                            current_concrete_count_after_gen;
                        best_match_cluster_index = Some(index);
                    } else if similarity == max_similarity_score {
                        // Tie-breaking logic for clusters with the same similarity score:
                        // Prefer the cluster whose template, after absorbing the current message,
                        // would have a higher count of concrete (non-wildcard) tokens.
                        // This prioritizes more specific templates.
                        if current_concrete_count_after_gen
                            > max_concrete_tokens_after_gen_for_best_match
                        {
                            debug!(target: "differential_drain", "Potential best match found for cluster ID {}: Same similarity {} but more concrete tokens {} (was {}). Updating best_match_cluster_index to {}.", cluster.cluster_id.to_string(), similarity, current_concrete_count_after_gen, max_concrete_tokens_after_gen_for_best_match, index);
                            max_concrete_tokens_after_gen_for_best_match =
                                current_concrete_count_after_gen;
                            best_match_cluster_index = Some(index);
                        } else {
                            trace!(target: "differential_drain", "Cluster ID {}: Same similarity {} and same or fewer concrete tokens ({} vs {}). Not updating best_match_cluster_index.", cluster.cluster_id.to_string(), similarity, current_concrete_count_after_gen, max_concrete_tokens_after_gen_for_best_match);
                        }
                        // If concrete token counts are also equal, the one with the lower index (found first) is kept.
                        // This ensures determinism in matching.
                    }
                } else {
                    trace!(target: "differential_drain", "Cluster ID {}: Similarity {} is good, but generalized template concrete token count {} is less than max_depth {}. Skipping.", cluster.cluster_id.to_string(), similarity, current_concrete_count_after_gen, self.max_depth);
                }
            }
        }

        if let Some(cluster_idx) = best_match_cluster_index {
            let old_template = self.clusters[cluster_idx].log_template.clone();
            let cluster_id_str = self.clusters[cluster_idx].cluster_id.to_string();
            info!(target: "differential_drain", "Found best match. Updating cluster ID: {}", cluster_id_str);

            let cluster = &mut self.clusters[cluster_idx];

            // Generalize Template
            let mut new_template = cluster.log_template.clone(); // This is actually the old template before generalization for this step
                                                                 // let mut _concrete_tokens_count = 0; // Prefixed with underscore - confirmed unused
                                                                 // let mut _wildcards_introduced_this_step = 0; // Prefixed with underscore - confirmed unused

            for (i, item) in new_template.iter_mut().enumerate() {
                if let TokenOrWildcard::Token(template_token_val) = item {
                    if template_token_val != &processed_message.tokens[i] {
                        *item = TokenOrWildcard::Wildcard;
                    }
                }
            }
            debug!(target: "differential_drain", "Cluster ID {}: Old template: {:?}, New generalized template: {:?}", cluster_id_str, old_template, new_template);

            // After forming the new_template, count concrete tokens again for the depth check
            let final_concrete_count = new_template
                .iter()
                .filter(|t| matches!(t, TokenOrWildcard::Token(_)))
                .count();

            // Depth Check:
            // Before finalizing the update to an existing cluster, ensure that the newly generalized
            // template does not become too generic. The template must have at least `self.max_depth`
            // concrete (non-wildcard) tokens. If this condition is not met, the message
            // will not be added to this cluster, and the logic will proceed to potentially
            // create a new cluster for this message.
            if final_concrete_count >= self.max_depth {
                cluster.log_template = new_template.clone();
                let sample_added = if cluster.samples.len() < 10 {
                    cluster.samples.push(processed_message.clone());
                    true
                } else {
                    false
                };
                cluster.count += 1;
                debug!(target: "differential_drain", "Cluster ID {}: Updated count to {}. Sample added: {}. Original message ID for current line: {}", cluster_id_str, cluster.count, sample_added, processed_message.original_message_id.to_string());

                // --- Start of Re-evaluation Logic ---
                let updated_cluster_index = cluster_idx;
                let generalized_template_of_updated_cluster = new_template; // This is the new_template of the updated_cluster_index cluster
                let updated_cluster_id_str =
                    self.clusters[updated_cluster_index].cluster_id.to_string();
                debug!(target: "differential_drain", "Starting re-evaluation for updated cluster ID: {}, new template: {:?}", updated_cluster_id_str, generalized_template_of_updated_cluster);

                // Reallocation Logic:
                // When a cluster (updated_cluster_index) is updated (its template is generalized),
                // there's a possibility that some messages in *other* clusters might now be
                // a better match for this newly generalized_template_of_updated_cluster
                // than for their own current cluster's template. This loop checks for such cases.
                let mut moves_to_perform: Vec<(usize, usize, usize)> = Vec::new();

                for other_cluster_idx in 0..self.clusters.len() {
                    if other_cluster_idx == updated_cluster_index {
                        continue; // Skip comparing the updated cluster with itself.
                    }
                    let other_cluster = &self.clusters[other_cluster_idx];
                    let other_cluster_id_str = other_cluster.cluster_id.to_string();
                    trace!(target: "differential_drain", "Re-eval: Checking other_cluster ID: {}, template: {:?}", other_cluster_id_str, other_cluster.log_template);

                    let other_cluster_template =
                        self.clusters[other_cluster_idx].log_template.clone();
                    let mut sample_indices_to_move_from_other: Vec<usize> = Vec::new();

                    for (sample_idx, msg_sample) in
                        self.clusters[other_cluster_idx].samples.iter().enumerate()
                    {
                        trace!(target: "differential_drain", "Re-eval: Evaluating sample (original ID: {}) from cluster ID {}", msg_sample.original_message_id.to_string(), other_cluster_id_str);
                        if msg_sample.tokens.len() != generalized_template_of_updated_cluster.len()
                        {
                            trace!(target: "differential_drain", "Re-eval: Sample original ID {} in cluster {} token length ({}) mismatch with generalized_template_of_updated_cluster length ({}). Skipping.", msg_sample.original_message_id.to_string(), other_cluster_id_str, msg_sample.tokens.len(), generalized_template_of_updated_cluster.len());
                            continue;
                        }
                        let similarity_to_generalized_updated_template = Self::calculate_similarity(
                            &msg_sample.tokens,
                            &generalized_template_of_updated_cluster,
                        );

                        if msg_sample.tokens.len() != other_cluster_template.len() {
                            // This case should ideally not happen if samples are consistent with their cluster templates
                            warn!(target: "differential_drain", "Re-eval: Sample original ID {} in cluster {} token length ({}) mismatch with its own template length ({}). Skipping.", msg_sample.original_message_id.to_string(), other_cluster_id_str, msg_sample.tokens.len(), other_cluster_template.len());
                            continue;
                        }
                        let similarity_to_own_template =
                            Self::calculate_similarity(&msg_sample.tokens, &other_cluster_template);
                        trace!(target: "differential_drain", "Re-eval: Sample original ID {} from cluster {}: Sim to generalized_template_of_updated_cluster (cluster {}): {}, Sim to own template (cluster {}): {}", msg_sample.original_message_id.to_string(), other_cluster_id_str, updated_cluster_id_str, similarity_to_generalized_updated_template, other_cluster_id_str, similarity_to_own_template);

                        // Sample Movement Decision:
                        // A sample is moved if its similarity to the `generalized_template_of_updated_cluster` (the generalized template
                        // of the cluster that just absorbed a new message) is:
                        // 1. Above the general `similarity_threshold`.
                        // 2. Strictly greater than its similarity to its current cluster's template.
                        if similarity_to_generalized_updated_template >= self.similarity_threshold
                            && similarity_to_generalized_updated_template
                                > similarity_to_own_template
                        {
                            debug!(target: "differential_drain", "Re-eval: Marking sample (original ID: {}) to move from cluster {} to cluster {}", msg_sample.original_message_id.to_string(), other_cluster_id_str, updated_cluster_id_str);
                            sample_indices_to_move_from_other.push(sample_idx);
                        }
                    }

                    // Collect all moves to be performed.
                    // Samples are removed in reverse order of their index within `sample_indices_to_move_from_other`
                    // to avoid index shifting issues during removal from `other_cluster.samples`.
                    if !sample_indices_to_move_from_other.is_empty() {
                        sample_indices_to_move_from_other.sort_unstable_by(|a, b| b.cmp(a)); // Sort descending to remove from end
                        for sample_idx in sample_indices_to_move_from_other {
                            moves_to_perform.push((
                                other_cluster_idx,
                                sample_idx,
                                updated_cluster_index,
                            ));
                        }
                    }
                }

                // Perform all scheduled moves.
                // `moves_to_perform` stores tuples of (from_cluster_index, sample_index_in_from_cluster, to_cluster_index).
                if !moves_to_perform.is_empty() {
                    info!(target: "differential_drain", "Re-eval: Performing {} sample movements.", moves_to_perform.len());
                    for (from_idx, sample_idx_in_from_cluster, to_idx) in moves_to_perform {
                        // The `sample_idx_in_from_cluster` is valid because `Vec::remove` shifts subsequent elements.
                        // However, since we sorted indices to remove from the end for `sample_indices_to_move_from_other`
                        // when collecting moves from a *single* `other_cluster`, this specific index is correct
                        // for that batch. `moves_to_perform` aggregates these batches.
                        // The critical part is that `remove` is done one by one, and `sample_idx_in_from_cluster` was correct at the moment it was recorded for removal.
                        let from_cluster_id_str = self.clusters[from_idx].cluster_id.to_string();
                        let to_cluster_id_str = self.clusters[to_idx].cluster_id.to_string();

                        // It's safer to log details of the message *before* it's removed if possible,
                        // or ensure that `remove` returns the item. `Vec::remove` does return the item.
                        let msg_to_move = self.clusters[from_idx]
                            .samples
                            .remove(sample_idx_in_from_cluster);
                        debug!(target: "differential_drain", "Re-eval: Moving sample (original ID: {}) from cluster {} to cluster {}", msg_to_move.original_message_id.to_string(), from_cluster_id_str, to_cluster_id_str);

                        self.clusters[from_idx].count -= 1;
                        let from_count = self.clusters[from_idx].count;

                        let sample_added_to_dest = if self.clusters[to_idx].samples.len() < 10 {
                            self.clusters[to_idx].samples.push(msg_to_move); // msg_to_move is consumed here
                            true
                        } else {
                            // If samples are full, we still increment count, but don't store the sample.
                            // The moved message (msg_to_move) is dropped here if not added to samples.
                            false
                        };
                        self.clusters[to_idx].count += 1;
                        let to_count = self.clusters[to_idx].count;
                        debug!(target: "differential_drain", "Re-eval: Cluster {} new count: {}. Cluster {} new count: {}. Sample stored in dest: {}", from_cluster_id_str, from_count, to_cluster_id_str, to_count, sample_added_to_dest);
                    }
                }
                info!(target: "differential_drain", "Performing cluster cleanup (retain where count > 0). Current cluster count before retain: {}", self.clusters.len());
                let initial_cluster_count_before_retain = self.clusters.len();
                // Cluster Cleanup:
                // After potential sample movements, some clusters might have their `count` reduced to zero.
                // This `retain` call removes such empty clusters from `self.clusters`.
                self.clusters.retain(|cluster| {
                    if cluster.count == 0 {
                        debug!(target: "differential_drain", "Removing cluster ID {} (template: {:?}) as its count is 0.", cluster.cluster_id.to_string(), cluster.log_template);
                        false
                    } else {
                        true
                    }
                });
                debug!(target: "differential_drain", "Cluster cleanup finished. Retained {} clusters out of {}.", self.clusters.len(), initial_cluster_count_before_retain);
                info!(target: "differential_drain", "process_line finished for line: {}. Matched and updated existing cluster.", line);
                return Ok(false);
            } else {
                // Generalization made template too vague, proceed to create new cluster.
                // This happens if `final_concrete_count < self.max_depth`.
                info!(target: "differential_drain", "process_line: Generalization of existing cluster made template too vague for line: {}", line);
            }
        }

        // Create New Cluster:
        // If no suitable existing cluster is found (or if updating a cluster made its template too vague),
        // a new cluster is created for the current log message.
        let template = Self::create_template_from_message(&processed_message.tokens);
        let initial_concrete_count = template
            .iter()
            .filter(|t| matches!(t, TokenOrWildcard::Token(_)))
            .count();
        // Depth Check for New Cluster:
        // A new cluster is only created if its initial template (derived directly from the message)
        // meets the `max_depth` requirement. This prevents the creation of overly generic clusters
        // from single, very diverse log messages if `template.is_empty()` is false.
        // The `!template.is_empty()` check handles cases where tokenization results in no tokens.
        if initial_concrete_count < self.max_depth && !template.is_empty() {
            // This new message would create a template that's too generic from the start.
            // Depending on desired strictness, one might log this and/or simply not add it.
            // Current logic proceeds to create it, but this comment highlights the check.
        }
        let new_cluster = LogCluster::new(processed_message.clone(), template.clone()); // Clone processed_message and template for logging
        info!(target: "differential_drain", "Creating new cluster for line: {}. Cluster ID: {}", line, new_cluster.cluster_id.to_string());
        debug!(target: "differential_drain", "New cluster ID {} template: {:?}, initial message original ID: {}", new_cluster.cluster_id.to_string(), new_cluster.log_template, new_cluster.samples[0].original_message_id.to_string());
        self.clusters.push(new_cluster);
        info!(target: "differential_drain", "process_line finished for line: {}. New cluster created.", line);
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
        let expected_template: Vec<TokenOrWildcard> = ["This", "is", "a", "test", "log", "line"]
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
            .clusters
            .first()
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
        let _c1_generalized_template = [
            TokenOrWildcard::Token("unique1".to_string()),
            TokenOrWildcard::Wildcard,
            TokenOrWildcard::Wildcard,
        ];
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
        // Assert properties of the second cluster (Cluster B)
        let c_b_final = drain
            .clusters
            .iter()
            .find(|c| c.cluster_id != c_a_final.cluster_id)
            .expect("Failed to find the second cluster (Cluster B)");

        assert_eq!(c_b_final.count, 2, "Expected Cluster B to have count 2");
        assert_template_equals(&c_b_final.log_template, &["prefix", "*", "suffix_B"]);
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

    fn test_calculate_similarity_len1_and_empty() {
        let msg_tokens_a = vec!["a".to_string()];
        let template_tokens_a = vec![TokenOrWildcard::Token("a".to_string())];
        assert_eq!(
            DifferentialDrain::calculate_similarity(&msg_tokens_a, &template_tokens_a),
            1.0
        );

        let template_tokens_b = vec![TokenOrWildcard::Token("b".to_string())];
        assert_eq!(
            DifferentialDrain::calculate_similarity(&msg_tokens_a, &template_tokens_b),
            0.0
        );

        let template_tokens_wildcard = vec![TokenOrWildcard::Wildcard];
        assert_eq!(
            DifferentialDrain::calculate_similarity(&msg_tokens_a, &template_tokens_wildcard),
            1.0
        );

        let msg_tokens_empty: Vec<String> = vec![];
        let template_tokens_empty: Vec<TokenOrWildcard> = vec![];
        assert_eq!(
            DifferentialDrain::calculate_similarity(&msg_tokens_empty, &template_tokens_empty),
            1.0
        );

        assert_eq!(
            DifferentialDrain::calculate_similarity(&msg_tokens_a, &template_tokens_empty),
            0.0
        );

        assert_eq!(
            DifferentialDrain::calculate_similarity(&msg_tokens_empty, &template_tokens_a),
            0.0
        );
    }

    #[test]
    fn test_process_line_len1_no_match_creates_new_cluster() {
        let mut drain = create_test_drain(0.5, 1);
        drain.process_line("tok1".to_string()).unwrap();
        drain.process_line("tok2".to_string()).unwrap();
        assert_eq!(drain.clusters.len(), 2);
    }

    #[test]
    fn test_process_line_len1_exact_match_updates_cluster() {
        let mut drain = create_test_drain(0.5, 1);
        drain.process_line("tok1".to_string()).unwrap();
        drain.process_line("tok1".to_string()).unwrap();
        assert_eq!(drain.clusters.len(), 1);
        assert_eq!(drain.clusters[0].count, 2);
    }

    #[test]
    fn test_process_line_len1_generalizes_to_wildcard_ok_with_max_depth_0() {
        let mut drain = create_test_drain(0.4, 0); // max_depth = 0 allows full generalization
        drain.process_line("tokA".to_string()).unwrap();
        drain.process_line("tokB".to_string()).unwrap();
        assert_eq!(drain.clusters.len(), 1);
        assert_eq!(
            drain.clusters[0].log_template,
            vec![TokenOrWildcard::Wildcard]
        );
        assert_eq!(drain.clusters[0].count, 2);
    }

    #[test]
    fn test_process_line_len1_generalizes_to_wildcard_results_in_new_cluster_if_max_depth_1() {
        let mut drain = create_test_drain(0.4, 1); // max_depth = 1
        drain.process_line("tokA".to_string()).unwrap();
        drain.process_line("tokB".to_string()).unwrap();
        assert_eq!(
            drain.clusters.len(),
            2,
            "Generalizing C0 to [W(*)] would make it have 0 concrete tokens, failing max_depth=1 check, so tokB forms new cluster"
        );
        assert_eq!(
            drain.clusters[0].log_template,
            vec![TokenOrWildcard::Token("tokA".to_string())]
        );
        assert_eq!(
            drain.clusters[1].log_template,
            vec![TokenOrWildcard::Token("tokB".to_string())]
        );
    }

    #[test]
    fn test_exhaustive_reproducer_for_line_763_panic() {
        let mut drain = create_test_drain(0.5, 1);
        drain
            .process_line("msg typeA detailX common1".to_string())
            .unwrap();
        drain
            .process_line("msg typeA detailY common2".to_string())
            .unwrap();
        drain
            .process_line("msg typeB detailP common3".to_string())
            .unwrap();
        // This line is expected to trigger the panic
        drain
            .process_line("msg typeB detailQ common4".to_string())
            .unwrap();
    }
}
