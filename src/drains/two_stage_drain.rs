// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::{collections::HashMap, sync::Arc};

use anyhow::{anyhow, Error};
use uuid::Uuid;
use fraction::{BigInt, FromPrimitive, Ratio};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use regex::Regex;
use string_interner::{DefaultSymbol, StringInterner};

use crate::log_group::LogGroup;
use crate::record::Record;
use crate::record::tokens::ASTERISK;

// Using the same INTERNER as simple.rs for now.
// This might need to be re-evaluated if TwoStageDrain has different string interning needs.
lazy_static! {
    pub(crate) static ref INTERNER: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>> =
        Arc::new(RwLock::new(StringInterner::<
            string_interner::backend::BucketBackend,
        >::new()));
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    Leaf(Vec<LogGroup>),
    Internal(HashMap<DefaultSymbol, Node>),
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: Uuid,
    pub kind: NodeKind,
}

impl Node {
    pub fn new_internal_node() -> Self {
        Node {
            id: Uuid::new_v4(),
            kind: NodeKind::Internal(HashMap::new()),
        }
    }

    pub fn new_leaf_node() -> Self {
        Node {
            id: Uuid::new_v4(),
            kind: NodeKind::Leaf(Vec::new()),
        }
    }

    pub fn new_leaf_node_with_groups(groups: Vec<LogGroup>) -> Self {
        Node {
            id: Uuid::new_v4(),
            kind: NodeKind::Leaf(groups),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;
    use tracing_test::traced_test; // Ensure tracing-test is in dev-dependencies

    #[traced_test]
    #[test]
    fn test_new_drain() {
        assert_that(&TwoStageDrain::new(vec![], 0.5, 4, 10)).is_ok();
        assert_that(&TwoStageDrain::new(
            vec!["invalid-regex(".to_string()],
            0.5,
            4,
            10,
        ))
        .is_err();
    }

    #[traced_test]
    #[test]
    fn test_process_empty_line() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10).unwrap();
        assert_that(&drain.process_line("".to_string())).is_ok_containing(false);
    }

    #[traced_test]
    #[test]
    fn test_process_line_creates_new_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10).unwrap();
        assert_that(&drain.process_line("This is a test log line".to_string()))
            .is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_process_line_matches_existing_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 10, 10).unwrap(); // Increased max_depth for this test
        let _ = drain.process_line("Log message type A value1".to_string());
        assert_that(&drain.process_line("Log message type A value2".to_string()))
            .is_ok_containing(false);
    }

    #[traced_test]
    #[test]
    fn test_process_line_creates_second_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10).unwrap();
        let _ = drain.process_line("Log message type A value1".to_string());
        assert_that(
            &drain.process_line("Completely different log message valueX".to_string()),
        )
        .is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_preprocessing_replaces_domain_pattern() {
        let domain_regex = vec!["\\d{4}-\\d{2}-\\d{2}".to_string()]; // Date pattern
        let mut drain = TwoStageDrain::new(domain_regex, 0.5, 4, 10).unwrap();

        let line1 = "2023-10-26 This is a log".to_string();
        assert_that(&drain.process_line(line1)).is_ok_containing(true);

        // Verify the created log group's template
        // Expected preprocessed line: "<*> This is a log"
        // Tokenize this expected line to get its length and first token for tree traversal
        let expected_tokens: Vec<DefaultSymbol> = {
            let mut interner = INTERNER.write();
            " <*> This is a log" // Note: split_ascii_whitespace will handle the leading space if any
                .trim() // Ensure no leading/trailing whitespace affects tokenization for lookup
                .split_ascii_whitespace()
                .map(|s| interner.get_or_intern(s))
                .collect()
        };
        let expected_length = expected_tokens.len();
        let first_token_of_expected = expected_tokens[0];

        let root_node = drain.tree.get(&expected_length).unwrap();
        match &root_node.kind {
            NodeKind::Internal(children_map) => {
                let child_node = children_map.get(&first_token_of_expected).unwrap();
                match &child_node.kind {
                    NodeKind::Leaf(log_groups_vec) => {
                        assert_that(&log_groups_vec.len()).is_equal_to(1);
                        let log_group = &log_groups_vec[0];
                        // The event in LogGroup is already tokenized with wildcards.
                        // We need to compare its string representation.
                        assert_that(&log_group.event().to_string()).is_equal_to("<*> This is a log".to_string());
                    }
                    _ => panic!("Child node should be a Leaf for this test structure"),
                }
            }
            _ => panic!("Root node should be Internal for this test structure"),
        }

        let line2 = "2024-01-01 This is a log".to_string();
        assert_that(&drain.process_line(line2)).is_ok_containing(false);
    }
}

#[derive(Debug, Clone)]
pub struct TwoStageDrain {
    pub domain: Vec<Regex>,
    pub tree: HashMap<usize, Node>,
    pub threshold: Ratio<BigInt>,
    pub strings: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>>,
    pub max_depth: usize,
    pub max_children: usize,
    pub line_count_processed: usize, // Added for integration test harness
}

impl TwoStageDrain {
    fn collect_log_groups_from_node(
        &self, // Changed to &self as it doesn't modify TwoStageDrain itself
        node: &mut Node,
        all_log_groups: &mut Vec<LogGroup>,
    ) {
        match node.kind {
            NodeKind::Leaf(ref mut groups) => {
                all_log_groups.append(&mut std::mem::take(groups));
            }
            NodeKind::Internal(ref mut children_map) => {
                for child_node in children_map.values_mut() {
                    self.collect_log_groups_from_node(child_node, all_log_groups);
                }
            }
        }
    }

    fn get_or_create_log_group_mut(
        &mut self,
        current_node: &mut Node,
        record_tokens: &[DefaultSymbol], // Changed to slice
        current_depth: usize,
    ) -> &mut Vec<LogGroup> { // Return type changed to mutable ref
        // If current_depth + 1 > self.max_depth, this node MUST be a leaf node.
        if current_depth + 1 >= self.max_depth {
            match current_node.kind {
                NodeKind::Internal(ref mut children_map) => {
                    // This case implies we were an internal node but need to become a leaf.
                    // Or, if there's a specific child that IS a leaf node already.
                    // For DRAIN, if we hit max_depth, the *current* node's children *must* be leaves.
                    // This logic might need refinement: DRAIN creates a leaf when a new path segment at max_depth is added.
                    // The current token for this depth determines the child.
                    let token_for_this_depth = record_tokens.get(current_depth).cloned().unwrap_or_else(|| *ASTERISK); // Use ASTERISK if out of bounds

                    let leaf_node = children_map
                        .entry(token_for_this_depth)
                        .or_insert_with(Node::new_leaf_node);
                    
                    // Ensure it's a leaf, convert if necessary (though or_insert_with should handle it)
                    if !matches!(leaf_node.kind, NodeKind::Leaf(_)) {
                        *leaf_node = Node::new_leaf_node();
                    }

                    match leaf_node.kind {
                        NodeKind::Leaf(ref mut log_groups) => {
                            return log_groups;
                        }
                        _ => unreachable!(), // Should have been converted to Leaf
                    }
                }
                NodeKind::Leaf(ref mut log_groups) => {
                    // Already a leaf node at max_depth (or before)
                    return log_groups;
                }
            }
        }

        // If not at max_depth, we are dealing with an Internal node (or converting a Leaf to Internal)
        if let NodeKind::Leaf(ref mut log_groups) = &mut current_node.kind {
            // Need to convert this Leaf to an Internal node because we are not yet at max_depth.
            // This happens if a path was shorter previously and is now being extended.
            if !log_groups.is_empty() {
                // If there are existing log groups, they become children of a new '*' node,
                // as the current path is being extended with more specific tokens.
                // This is a simplification; DRAIN might push them down based on the next token
                // of their original template. For now, a simple '*' branch.
                let mut new_children_map = HashMap::new();
                let old_groups = std::mem::take(log_groups); // Take ownership of the groups
                new_children_map.insert(*ASTERISK, Node::new_leaf_node_with_groups(old_groups)); // Requires new_leaf_node_with_groups
                current_node.kind = NodeKind::Internal(new_children_map);
            } else {
                 current_node.kind = NodeKind::Internal(HashMap::new());
            }
        }
        
        // Now we are sure current_node.kind is Internal.
        // This outer match is to satisfy the borrow checker, as we might change current_node.kind.
        if let NodeKind::Internal(children_map) = &mut current_node.kind {
            let token_for_this_depth = record_tokens.get(current_depth).cloned().unwrap_or_else(|| *ASTERISK);

            // Max children rule:
            // If the node is full and we are trying to insert a new token (not an existing one)
            if children_map.len() >= self.max_children && !children_map.contains_key(&token_for_this_depth) {
                let mut collected_groups = Vec::new();
                // Iterate over a temporary collection of values to allow modification of children_map
                // This is a bit tricky as collect_log_groups_from_node takes &mut Node.
                // We can take the children_map out, iterate, then put it back if needed,
                // or directly change the node kind.
                
                let mut old_children_map = std::mem::take(children_map); // Take the map out
                for child_node in old_children_map.values_mut() {
                    self.collect_log_groups_from_node(child_node, &mut collected_groups);
                }
                // children_map is now empty here as it was taken. The old_children_map will be dropped.
                
                current_node.kind = NodeKind::Leaf(collected_groups);
                // Now that current_node is a Leaf, we need to return its groups.
                // This subsequent match will hit the Leaf arm.
            } else {
                 // If max_children rule not triggered, proceed with normal traversal/insertion.
                let child_node = children_map
                    .entry(token_for_this_depth)
                    .or_insert_with(Node::new_internal_node);
                return self.get_or_create_log_group_mut(child_node, record_tokens, current_depth + 1);
            }
        }

        // After potential modification (e.g. to Leaf due to max_children), match again to get the groups.
        // This handles the case where the node was converted to Leaf above, or was already Leaf from max_depth.
        match &mut current_node.kind {
            NodeKind::Leaf(ref mut log_groups) => {
                return log_groups;
            }
            NodeKind::Internal(_) => {
                 // This path should ideally not be hit if the logic above correctly returns after recursive call
                 // or converts to leaf and then returns from the Leaf match.
                 // However, if max_children was met, and the node became a leaf, this internal arm should not be hit.
                 // If max_children was NOT met, the recursive call should have returned.
                 // This could indicate an issue if token_for_this_depth *was* present, but we didn't recurse.
                 // For safety, let's assume if we are here, it's because we need to get child based on current token
                 // (this would be redundant if the above 'else' block for non-max_children is correct).
                 // This part of the logic flow might need careful review.
                 // The prompt implies the 'else' branch handles the non-conversion case by recursing and returning.
                 // So, if we reach here, it must be that the node was converted to Leaf.
                unreachable!("Node should either be a Leaf, or the function should have returned from recursive call within Internal");
            }
        }
    }

    pub fn new(
        domain_regex_strings: Vec<String>,
        threshold: f32,
        max_depth: usize,
        max_children: usize,
    ) -> Result<Self, Error> {
        let domain_patterns = domain_regex_strings
            .iter()
            .map(|s| Regex::new(s))
            .collect::<Result<Vec<Regex>, regex::Error>>()?;

        let threshold_ratio = Ratio::from_float::<f32>(threshold)
            .ok_or_else(|| anyhow!("Invalid threshold value: {} cannot be converted to a Ratio", threshold))?;

        Ok(Self {
            domain: domain_patterns,
            tree: HashMap::new(),
            threshold: threshold_ratio,
            strings: INTERNER.clone(),
            max_depth,
            max_children,
            line_count_processed: 0, // Initialize new field
        })
    }

    pub fn process_line(&mut self, line: String) -> Result<bool, Error> {
        if line.is_empty() {
            return Ok(false);
        }

        let mut processed_line = line;
        for re in &self.domain {
            processed_line = re.replace_all(&processed_line, "<*>").into_owned();
        }

        if processed_line.is_empty() || processed_line == "<*>" {
            return Ok(false);
        }
        
        // Increment counter for successfully processed lines (past initial checks)
        // self.line_count_processed += 1; // Moved this to after tokenization check

        let new_record = Record::new(processed_line.clone()); // Clone processed_line for Record

        // Tokenize the processed line string for tree traversal
        let processed_line_tokens: Vec<DefaultSymbol> = {
            let mut interner = INTERNER.write();
            processed_line
                .split_ascii_whitespace()
                .map(|s| interner.get_or_intern(s))
                .collect()
        }; // Lock released here

        if processed_line_tokens.is_empty() {
            return Ok(false); // Or handle as an error/empty line
        }
        
        self.line_count_processed += 1; // Increment after tokenization is confirmed non-empty

        let length = processed_line_tokens.len();

        // Get the root node for this length.
        let root_node_for_length = self.tree.entry(length).or_insert_with(Node::new_internal_node);

        // Get the mutable reference to the log group vector for this record.
        let log_groups_vec = self.get_or_create_log_group_mut(
            root_node_for_length,
            &processed_line_tokens,
            0,
        );

        // Perform log group matching.
        if log_groups_vec.is_empty() {
            log_groups_vec.push(LogGroup::new(new_record));
            Ok(true)
        } else {
            let mut best_match_score = 0;
            let mut best_match_idx: Option<usize> = None;

            for (idx, group) in log_groups_vec.iter_mut().enumerate() {
                let current_score = new_record.calc_sim_score(group.event());
                if current_score > best_match_score {
                    best_match_score = current_score;
                    best_match_idx = Some(idx);
                }
            }

            let score_ratio =
                Ratio::<BigInt>::new(BigInt::from(best_match_score), BigInt::from(length));

            if best_match_idx.is_some() && score_ratio > self.threshold {
                if let Some(idx) = best_match_idx {
                    log_groups_vec[idx].add_example(new_record);
                    Ok(false)
                } else {
                     // Should not happen if best_match_idx is Some
                    unreachable!();
                }
            } else {
                log_groups_vec.push(LogGroup::new(new_record));
                Ok(true)
            }
        }
    }
}
