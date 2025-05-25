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
use fraction::{BigInt, Ratio}; // Removed ToPrimitive
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
        // First line should create a new group
        assert_that(&drain.process_line(line1)).is_ok_containing(true);

        // Second line, differing only in the preprocessed part, should match the existing group.
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
    // Added lifetime 'a to the method itself, not the impl block,
    // to tie current_node's lifetime to candidate_groups.
    fn find_candidate_log_groups<'a>(
        &self, // Takes &self because it only reads the tree structure initially
        current_node: &'a Node, // The node to search from, with lifetime 'a
        record_tokens: &[DefaultSymbol],
        current_depth: usize,
        max_depth: usize, // Pass max_depth
        candidate_groups: &mut Vec<&'a LogGroup>, // Collects references with lifetime 'a
    ) {
        if current_depth >= max_depth { // Base case: if we're at or beyond max_depth, this node should be a leaf
            if let NodeKind::Leaf(log_groups) = &current_node.kind {
                for lg in log_groups {
                    candidate_groups.push(lg);
                }
            }
            return;
        }

        match &current_node.kind {
            NodeKind::Leaf(log_groups) => {
                // If we encounter a leaf node before reaching max_depth, add its groups
                for lg in log_groups {
                    candidate_groups.push(lg);
                }
            }
            NodeKind::Internal(children_map) => {
                // Path 1: Match specific token
                if let Some(token_for_this_depth) = record_tokens.get(current_depth) {
                    if let Some(child_node) = children_map.get(token_for_this_depth) {
                        self.find_candidate_log_groups(
                            child_node,
                            record_tokens,
                            current_depth + 1,
                            max_depth,
                            candidate_groups,
                        );
                    }
                }

                // Path 2: Match wildcard token <*>
                if let Some(wildcard_child_node) = children_map.get(&ASTERISK) {
                    self.find_candidate_log_groups(
                        wildcard_child_node,
                        record_tokens,
                        current_depth + 1,
                        max_depth,
                        candidate_groups,
                    );
                }
            }
        }
    }
    
    fn collect_log_groups_from_node( // Removed &self
        node: &mut Node,
        all_log_groups: &mut Vec<LogGroup>,
    ) {
        match node.kind {
            NodeKind::Leaf(ref mut groups) => {
                all_log_groups.append(&mut std::mem::take(groups));
            }
            NodeKind::Internal(ref mut children_map) => {
                for child_node in children_map.values_mut() {
                    // Note: Recursive call needs to be Self:: or TwoStageDrain:: if it's a static method
                    // For now, assuming it's made a free function or handled appropriately.
                    // If it remains an instance method (even without using &self), this call needs care.
                    // Let's assume it's called as a static-like method or free function for now.
                    Self::collect_log_groups_from_node(child_node, all_log_groups);
                }
            }
        }
    }

    // get_or_create_log_group_mut no longer needs &mut self
    // It operates on current_node and global INTERNER, and parameters.
    // However, to be callable from process_line which has &mut self,
    // and to fit the overall structure without a larger refactor now,
    // we keep &mut self but acknowledge it's not strictly needed for E0499 if INTERNER is global.
    // The E0499 fix is primarily about how `self.tree` (via `root_node_for_length`)
    // and `self` (for the method call) are borrowed.
    // By making collect_log_groups_from_node not take &self, we simplify one part.
    // The core issue is that `root_node_for_length` is a mutable borrow from `self.tree`.
    // Then `get_or_create_log_group_mut` is called on `self`.
    //
    // If get_or_create_log_group_mut did NOT take &mut self, the E0499 would be resolved.
    // But it needs to, to call collect_log_groups_from_node if that method remains on &self.
    // Since we changed collect_log_groups_from_node to not need &self,
    // get_or_create_log_group_mut also doesn't strictly need &mut self IF
    // it didn't modify anything else on self (like self.strings, which it doesn't directly).
    //
    // Let's proceed with the change to `collect_log_groups_from_node` first as it's cleaner.
    // The E0499 might persist if `get_or_create_log_group_mut` still takes `&mut self`.
    // The true fix for E0499 is to break the aliasing of `&mut self.tree` (via `root_node_for_length`)
    // and `&mut self` (for the method call).
    // This often involves:
    // 1. Finishing the borrow of `root_node_for_length` before calling the method.
    // 2. Restructuring the method to not take `&mut self` if it only operates on its arguments and globals.
    //
    // For now, only applying the `collect_log_groups_from_node` change and `threshold.clone()`.
    // The E0499 error is more complex and might require a different strategy if it persists.

    fn get_or_create_log_group_mut<'a>(
        // &mut self, // No longer takes &mut self
        current_node: &'a mut Node,
        record_tokens: &[DefaultSymbol], // Path to traverse/create
        current_depth: usize,
        max_depth: usize,
        max_children: usize,
    ) -> &'a mut Vec<LogGroup> {
        // Case 1: We've reached the target depth for the given record_tokens path
        if current_depth == record_tokens.len() {
            // Ensure it's a leaf node. If not, it becomes an empty leaf.
            // This is a simplification; DRAIN paper (Sec 3.2, Step 3 & Fig 3) suggests new leaf nodes are added.
            // If it's an internal node with existing children, this conversion is lossy for those children.
            // This part needs careful consideration against DRAIN's exact specification for adding groups
            // when a path that was previously a prefix now becomes a cluster location.
            // The current `find_candidate_log_groups` should ideally return paths that end at existing leaves
            // or at points where new leaves should be created.
            if let NodeKind::Internal(children) = &current_node.kind {
                 if children.is_empty() { // Safe to convert if no actual children exist
                    current_node.kind = NodeKind::Leaf(Vec::new());
                } else {
                    // Path ends, but it's an internal node with children. This is a conflict.
                    // DRAIN implies a log group can't exist *at* an internal node.
                    // A robust solution would involve creating a special child (e.g., under an <END> token)
                    // to hold the log groups for this exact path if this node must remain internal.
                    // For now, to satisfy returning a &mut Vec<LogGroup> from *this* node,
                    // we'd have to convert it, potentially orphaning children.
                    // This indicates that the path provided to this function might sometimes need
                    // to be extended by one more (special) token to correctly get/create a leaf.
                    // However, the prompt's logic forces it to become a leaf.
                    // This might be acceptable if `find_candidate_log_groups` ensures this path leads to a "true" leaf.
                    // If a candidate path from `find_candidate_log_groups` points here, it expects a group here.
                    // For simplicity as per prompt, convert to Leaf.
                    // Consider this a point for future refinement based on DRAIN paper details.
                    current_node.kind = NodeKind::Leaf(Vec::new()); // Simplification: becomes an empty leaf
                }
            } else if !matches!(current_node.kind, NodeKind::Leaf(_)) {
                 // If it's neither Internal nor Leaf (impossible), or to ensure it is Leaf.
                 *current_node = Node::new_leaf_node();
            }

            match &mut current_node.kind {
                NodeKind::Leaf(log_groups) => return log_groups,
                _ => unreachable!(), // Should be a Leaf now
            }
        }

        // Case 2: Max depth reached. Node must be a leaf.
        if current_depth + 1 >= max_depth {
            if let NodeKind::Internal(ref mut children_map_taken) = current_node.kind {
                let mut collected_groups = Vec::new();
                let mut temp_children_map = std::mem::take(children_map_taken);
                for child_node in temp_children_map.values_mut() {
                    // collect_log_groups_from_node doesn't take &self anymore
                    Self::collect_log_groups_from_node(child_node, &mut collected_groups);
                }
                *current_node = Node::new_leaf_node_with_groups(collected_groups);
            }
            // Ensure it is a Leaf.
            if !matches!(current_node.kind, NodeKind::Leaf(_)) {
                 *current_node = Node::new_leaf_node(); // Should not happen if logic above is correct
            }
            match &mut current_node.kind {
                NodeKind::Leaf(ref mut log_groups) => return log_groups,
                _ => unreachable!("Should be a Leaf node at max_depth"),
            }
        }

        // Case 3: Not at max depth, and path is not exhausted. Traverse/create internal nodes.
        if let NodeKind::Leaf(ref mut log_groups) = &mut current_node.kind {
            let mut new_children_map = HashMap::new();
            if !log_groups.is_empty() {
                let old_groups = std::mem::take(log_groups);
                new_children_map.insert(*ASTERISK, Node::new_leaf_node_with_groups(old_groups));
            }
            current_node.kind = NodeKind::Internal(new_children_map);
        }

        match &mut current_node.kind {
            NodeKind::Internal(children_map) => {
                let token_for_this_depth = record_tokens[current_depth];

                if children_map.len() >= max_children && !children_map.contains_key(&token_for_this_depth) {
                    let mut collected_groups = Vec::new();
                    let mut old_children_map = std::mem::take(children_map);
                    for child_node in old_children_map.values_mut() {
                        Self::collect_log_groups_from_node(child_node, &mut collected_groups);
                    }
                    *current_node = Node::new_leaf_node_with_groups(collected_groups);
                    match &mut current_node.kind {
                        NodeKind::Leaf(ref mut log_groups) => return log_groups,
                        _ => unreachable!("Converted to Leaf due to max_children"),
                    }
                } else {
                    let child_node = children_map
                        .entry(token_for_this_depth)
                        .or_insert_with(Node::new_internal_node);
                    return Self::get_or_create_log_group_mut(child_node, record_tokens, current_depth + 1, max_depth, max_children);
                }
            }
            NodeKind::Leaf(_) => {
                unreachable!("Node should have been converted to Internal if not at max_depth and path not exhausted");
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
        let local_max_depth = self.max_depth;
        let local_max_children = self.max_children;
        let local_threshold = self.threshold.clone(); // Fixed E0507 by cloning

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
        
        let new_record = Record::new(processed_line.clone()); // Clone processed_line for Record

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
        let root_node_for_length = self.tree.get(&length); // Immutable borrow for find_candidate_log_groups

        if let Some(root_node) = root_node_for_length {
            let mut candidate_groups: Vec<&LogGroup> = Vec::new();
            self.find_candidate_log_groups(
                root_node,
                &processed_line_tokens,
                0,
                local_max_depth, // Use the local variable
                &mut candidate_groups,
            );
            
            // TODO: Perform similarity search on candidate_groups.
            // If match found:
            //   Need to get the *mutable* LogGroup. This is the tricky part.
            //   Perhaps find_candidate_log_groups returns Vec<Uuid> of log groups,
            //   then we iterate, find best Uuid, then get_mut_log_group_by_id(uuid) to modify.
            //   For now, let's put a placeholder here.
            // if !candidate_groups.is_empty() { /* ... perform matching ... */ }
        } else {
            // No root node for this length, so definitely a new group.
            // This case will be handled when integrating the mutable part.
        }

        // Comment out the existing call to Self::get_or_create_log_group_mut(...) and subsequent logic.
        /*
        let root_node_for_length_mut = self.tree.entry(length).or_insert_with(Node::new_internal_node);
        let log_groups_vec = Self::get_or_create_log_group_mut(
            root_node_for_length_mut,
            &processed_line_tokens,
            0,
            local_max_depth,
            local_max_children,
        );

        if log_groups_vec.is_empty() {
            log_groups_vec.push(LogGroup::new(new_record));
            return Ok(true);
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
            
            if best_match_idx.is_some() && score_ratio > local_threshold {
                if let Some(idx) = best_match_idx {
                    log_groups_vec[idx].add_example(new_record);
                    return Ok(false);
                } else {
                    unreachable!();
                }
            } else {
                log_groups_vec.push(LogGroup::new(new_record));
                return Ok(true);
            }
        }
        */
        Ok(true) // Placeholder return
    }
}
