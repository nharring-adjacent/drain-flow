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
use fraction::{BigInt, Ratio}; // Removed ToPrimitive
use lazy_static::lazy_static;
use parking_lot::RwLock;
use regex::Regex;
use string_interner::{DefaultSymbol, StringInterner};
use uuid::Uuid;

use crate::drains::api::Drain;
use crate::log_group::LogGroup;
use crate::record::Record;
// Removed: use crate::record::tokens::ASTERISK;

// Use the shared interner from simple.rs
use crate::drains::simple;

lazy_static! {
    static ref DRAIN_ASTERISK: DefaultSymbol = simple::INTERNER.write().get_or_intern_static("<*>");
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

    // Removed collect_all_groups_in_subtree from impl Node

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
        assert_that(&Drain::process_line(&mut drain, "".to_string())).is_ok_containing(false);
    }

    #[traced_test]
    #[test]
    fn test_process_line_creates_new_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10).unwrap();
        assert_that(&Drain::process_line(&mut drain, "This is a test log line".to_string()))
            .is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_process_line_matches_existing_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 10, 10).unwrap(); // Increased max_depth for this test
        let _ = Drain::process_line(&mut drain, "Log message type A value1".to_string());
        assert_that(&Drain::process_line(&mut drain, "Log message type A value2".to_string()))
            .is_ok_containing(false);
    }

    #[traced_test]
    #[test]
    fn test_process_line_creates_second_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10).unwrap();
        let _ = Drain::process_line(&mut drain, "Log message type A value1".to_string());
        assert_that(&Drain::process_line(&mut drain, "Completely different log message valueX".to_string()))
            .is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_preprocessing_replaces_domain_pattern() {
        let domain_regex = vec!["\\d{4}-\\d{2}-\\d{2}".to_string()]; // Date pattern
        let mut drain = TwoStageDrain::new(domain_regex, 0.5, 4, 10).unwrap();

        let line1 = "2023-10-26 This is a log".to_string();
        // First line should create a new group
        assert_that(&Drain::process_line(&mut drain, line1)).is_ok_containing(true);

        // Second line, differing only in the preprocessed part, should match the existing group.
        let line2 = "2024-01-01 This is a log".to_string();
        assert_that(&Drain::process_line(&mut drain, line2)).is_ok_containing(false);
    }
}

// Define collect_all_groups_in_subtree_free as a free function
fn collect_all_groups_in_subtree_free(
    node: &Node,
    path_to_node: Vec<DefaultSymbol>,
    candidate_paths: &mut Vec<(Vec<DefaultSymbol>, Uuid)>,
) {
    match &node.kind {
        NodeKind::Leaf(log_groups) => {
            for lg in log_groups {
                candidate_paths.push((path_to_node.clone(), lg.id));
            }
        }
        NodeKind::Internal(children_map) => {
            for (token, child_node) in children_map.iter() {
                let mut path_to_child = path_to_node.clone();
                path_to_child.push(*token);
                collect_all_groups_in_subtree_free(child_node, path_to_child, candidate_paths);
            }
        }
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
    fn find_candidate_log_groups(
        &self,
        current_node: &Node,
        record_tokens: &[DefaultSymbol],
        current_path: Vec<DefaultSymbol>, // Path from root of this length-tree to current_node
        current_depth: usize,
        max_depth: usize,
        candidate_paths: &mut Vec<(Vec<DefaultSymbol>, Uuid)>, // path_to_leaf, log_group_id
    ) {
        // Base case: if we're at or beyond max_depth, this node should be a leaf
        // or treated as such for candidate collection.
        if current_depth >= max_depth {
            if let NodeKind::Leaf(log_groups) = &current_node.kind {
                for lg in log_groups {
                    candidate_paths.push((current_path.clone(), lg.id));
                }
            }
            return;
        }

        match &current_node.kind {
            NodeKind::Leaf(log_groups) => {
                // If we encounter a leaf node before reaching max_depth, add its groups
                for lg in log_groups {
                    candidate_paths.push((current_path.clone(), lg.id));
                }
            }
            NodeKind::Internal(children_map) => {
                let mut specific_token_path_followed = false;
                let mut wildcard_path_followed = false;
                let next_log_token_opt = record_tokens.get(current_depth);

                // Path 1: Attempt to follow specific token from the log message
                if let Some(token_val) = next_log_token_opt {
                    if let Some(child_node) = children_map.get(token_val) {
                        let mut next_path = current_path.clone();
                        next_path.push(*token_val);
                        self.find_candidate_log_groups(
                            child_node,
                            record_tokens,
                            next_path,
                            current_depth + 1,
                            max_depth,
                            candidate_paths,
                        );
                        specific_token_path_followed = true;
                    }
                }

                // Path 2: Attempt to follow wildcard token <*> in the tree,
                // but only if it's different from the specific token path (or if specific token is not a wildcard)
                // This check avoids double-counting if record_tokens[current_depth] is already DRAIN_ASTERISK
                let specific_token_is_wildcard =
                    next_log_token_opt.is_some_and(|t| *t == *DRAIN_ASTERISK);
                if !specific_token_is_wildcard {
                    // only try wildcard if specific token wasn't already the wildcard
                    if let Some(wildcard_child_node) = children_map.get(&*DRAIN_ASTERISK) {
                        let mut next_path_wildcard = current_path.clone();
                        next_path_wildcard.push(*DRAIN_ASTERISK);
                        self.find_candidate_log_groups(
                            wildcard_child_node,
                            record_tokens,
                            next_path_wildcard,
                            current_depth + 1,
                            max_depth,
                            candidate_paths,
                        );
                        wildcard_path_followed = true; // Mark that a wildcard path was taken
                    }
                } else {
                    // If specific token was already a wildcard, and we followed it,
                    // then specific_token_path_followed is true. We consider wildcard path as "followed".
                    if specific_token_path_followed {
                        wildcard_path_followed = true;
                    }
                }

                // Condition for collecting all subtree groups (DRAIN paper Step 2 adjustment)
                // If traversal for the current log's token sequence stops at this internal node
                // (i.e., neither the specific next token nor a wildcard led further down this path for *this sequence*)
                // then collect all groups under this node.
                // This implies next_log_token_opt.is_some() because we are at an Internal node and not at max_depth.
                if current_depth < max_depth
                    && next_log_token_opt.is_some()
                    && !specific_token_path_followed
                    && !wildcard_path_followed
                {
                    // No direct path continuation for the current log sequence. Collect all children.
                    for (token, child_node) in children_map.iter() {
                        let mut path_to_child = current_path.clone();
                        path_to_child.push(*token);
                        // Call the free function
                        collect_all_groups_in_subtree_free(
                            child_node,
                            path_to_child,
                            candidate_paths,
                        );
                    }
                }
            }
        }
    }

    // Helper to find a log group by ID starting from a given node (immutable search)
    fn find_log_group_in_node_by_id(node: &Node, group_id: Uuid) -> Option<&LogGroup> {
        match &node.kind {
            NodeKind::Leaf(log_groups) => {
                for lg in log_groups {
                    if lg.id == group_id {
                        return Some(lg);
                    }
                }
                None
            }
            NodeKind::Internal(children_map) => {
                for child_node in children_map.values() {
                    if let Some(lg) = Self::find_log_group_in_node_by_id(child_node, group_id) {
                        return Some(lg);
                    }
                }
                None
            }
        }
    }

    fn collect_log_groups_from_node(node: &mut Node, all_log_groups: &mut Vec<LogGroup>) {
        match node.kind {
            NodeKind::Leaf(ref mut groups) => {
                // This method is used when restructuring the tree (e.g. internal to leaf).
                // It takes ownership of the groups. For benchmarking, we need clones.
                all_log_groups.append(&mut std::mem::take(groups));
            }
            NodeKind::Internal(ref mut children_map) => {
                for child_node in children_map.values_mut() {
                    Self::collect_log_groups_from_node(child_node, all_log_groups);
                }
            }
        }
    }

    // New recursive helper for collect_all_log_groups (read-only traversal)
    fn collect_groups_recursive(node: &Node, collected_groups: &mut Vec<LogGroup>) {
        match &node.kind {
            NodeKind::Leaf(groups) => {
                for group in groups {
                    collected_groups.push(group.clone());
                }
            }
            NodeKind::Internal(children_map) => {
                for child_node in children_map.values() {
                    Self::collect_groups_recursive(child_node, collected_groups);
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
        current_node: &'a mut Node,
        record_tokens: &[DefaultSymbol],
        current_depth: usize,
        max_depth: usize,
        max_children: usize,
    ) -> &'a mut Vec<LogGroup> {
        // Case 1: Path is exhausted. Node must be a Leaf.
        if current_depth == record_tokens.len() {
            if !matches!(&current_node.kind, NodeKind::Leaf(_)) {
                // If it's Internal, or any other unexpected kind, convert to Leaf.
                // Orphaned children if it was Internal with children (simplified DRAIN logic).
                current_node.kind = NodeKind::Leaf(Vec::new());
            }
            // Now it's guaranteed to be a Leaf or made into one.
            match &mut current_node.kind {
                NodeKind::Leaf(log_groups) => return log_groups,
                _ => unreachable!("Node ensured to be Leaf for exhausted path."),
            }
        }

        // Case 2: Max depth reached. Node must become a Leaf.
        if current_depth + 1 >= max_depth {
            let final_groups_for_leaf =
                match std::mem::replace(&mut current_node.kind, NodeKind::Leaf(Vec::new())) {
                    NodeKind::Internal(mut children_map_taken) => {
                        let mut collected_groups = Vec::new();
                        for child_node in children_map_taken.values_mut() {
                            Self::collect_log_groups_from_node(child_node, &mut collected_groups);
                        }
                        collected_groups
                    }
                    NodeKind::Leaf(existing_groups) => existing_groups,
                    // No other kinds expected if Node is always constructed as Internal or Leaf.
                };
            current_node.kind = NodeKind::Leaf(final_groups_for_leaf);

            match &mut current_node.kind {
                NodeKind::Leaf(log_groups) => return log_groups,
                _ => unreachable!("Node converted to Leaf at max_depth."),
            }
        }

        // Case 3: Traverse/Create. Node kind might change. Loop to handle state transitions.
        // Temporary enum to hold action determined by initial inspection.
        enum DeterminedAction {
            Recurse, // Node is Internal and will remain Internal for recursion.
            TransformToLeaf(Vec<LogGroup>),
            TransformToInternal(HashMap<DefaultSymbol, Node>),
        }

        'transform_loop: loop {
            let action: DeterminedAction;

            // Phase 1: Decide action. This match takes a mutable borrow of current_node.kind.
            // If data is taken (e.g. via std::mem::take), that part of the borrow ends.
            match &mut current_node.kind {
                NodeKind::Internal(children_map) => {
                    let token_for_this_depth = record_tokens[current_depth];
                    if children_map.len() >= max_children
                        && !children_map.contains_key(&token_for_this_depth)
                    {
                        // Condition: Internal node is full and current token is not a child. Convert to Leaf.
                        let mut taken_map = std::mem::take(children_map); // children_map is now empty.
                        let mut collected_groups = Vec::new();
                        for (_symbol, node_in_map) in taken_map.iter_mut() {
                            // Iterate over the owned map
                            Self::collect_log_groups_from_node(node_in_map, &mut collected_groups);
                        }
                        action = DeterminedAction::TransformToLeaf(collected_groups);
                    } else {
                        // Decision is to recurse. No data taken yet.
                        action = DeterminedAction::Recurse;
                    }
                }
                NodeKind::Leaf(log_groups_vec) => {
                    // Node is Leaf, but path is not exhausted (Case 1 handled this). Convert Leaf to Internal.
                    let taken_groups = std::mem::take(log_groups_vec); // log_groups_vec is now empty.
                    let mut new_children_map = HashMap::new();
                    if !taken_groups.is_empty() {
                        new_children_map.insert(
                            *DRAIN_ASTERISK,
                            Node::new_leaf_node_with_groups(taken_groups),
                        );
                    }
                    action = DeterminedAction::TransformToInternal(new_children_map);
                }
            } // Mutable borrow of current_node.kind by the match ends here.

            // Phase 2: Execute action.
            match action {
                DeterminedAction::TransformToLeaf(groups) => {
                    current_node.kind = NodeKind::Leaf(groups);
                    // Loop again to re-evaluate current_node (now Leaf) against Case 1 or 2, or if path still not exhausted.
                    continue 'transform_loop;
                }
                DeterminedAction::TransformToInternal(map) => {
                    current_node.kind = NodeKind::Internal(map);
                    // Loop again to re-evaluate current_node (now Internal).
                    // Next iteration's Phase 1 will likely take the Recurse path.
                    continue 'transform_loop;
                }
                DeterminedAction::Recurse => {
                    // Re-borrow current_node.kind as Internal to get child for recursion.
                    // This is a new, distinct borrow.
                    if let NodeKind::Internal(children_map) = &mut current_node.kind {
                        let token_for_this_depth = record_tokens[current_depth];
                        let child_node = children_map
                            .entry(token_for_this_depth)
                            .or_insert_with(Node::new_internal_node);
                        return Self::get_or_create_log_group_mut(
                            child_node,
                            record_tokens,
                            current_depth + 1,
                            max_depth,
                            max_children,
                        );
                    } else {
                        // This state should ideally not be reached if 'Recurse' was determined when it was Internal.
                        // Implies current_node.kind changed unexpectedly or logic error.
                        unreachable!("Node kind was expected to be Internal for recursion.");
                    }
                }
            }
        } // end 'transform_loop
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

        let threshold_ratio = Ratio::from_float::<f32>(threshold).ok_or_else(|| {
            anyhow!(
                "Invalid threshold value: {} cannot be converted to a Ratio",
                threshold
            )
        })?;

        Ok(Self {
            domain: domain_patterns,
            tree: HashMap::new(),
            threshold: threshold_ratio,
            strings: simple::INTERNER.clone(), // Use shared interner
            max_depth,
            max_children,
            line_count_processed: 0, // Initialize new field
        })
    }
}

impl Drain for TwoStageDrain {
    fn process_line(&mut self, line: String) -> Result<bool, Error> {
        let local_max_depth = self.max_depth;
        let local_max_children = self.max_children;
        let local_threshold = self.threshold.clone();
        let local_domain = self.domain.clone();

        if line.is_empty() {
            return Ok(false);
        }

        let mut processed_line = line;
        for re in &local_domain {
            processed_line = re.replace_all(&processed_line, "<*>").into_owned();
        }

        if processed_line.is_empty() || processed_line == "<*>" {
            return Ok(false);
        }

        let new_record = Record::new(processed_line.clone());

        let processed_line_tokens: Vec<DefaultSymbol> = {
            let mut interner = self.strings.write(); // Use self.strings (shared interner)
            processed_line
                .split_ascii_whitespace()
                .map(|s| interner.get_or_intern(s))
                .collect()
        };

        if processed_line_tokens.is_empty() {
            return Ok(false);
        }

        self.line_count_processed += 1;
        let length = processed_line_tokens.len();

        // Immutable Phase: Find candidate log groups and best match
        let mut best_match_info: Option<(Vec<DefaultSymbol>, Uuid, u64)> = None; // path, id, score

        if let Some(root_node) = self.tree.get(&length) {
            let mut candidate_infos: Vec<(Vec<DefaultSymbol>, Uuid)> = Vec::new();
            // find_candidate_log_groups populates candidate_infos
            self.find_candidate_log_groups(
                root_node,
                &processed_line_tokens,
                Vec::new(), // Initial empty path for the root of this length-specific tree
                0,          // Initial depth
                local_max_depth,
                &mut candidate_infos,
            );

            for (path, group_id) in candidate_infos {
                // candidate_infos contains (path_to_leaf, log_group_id)
                // find_log_group_in_node_by_id needs to search from the same root_node
                if let Some(lg) = Self::find_log_group_in_node_by_id(root_node, group_id) {
                    let current_score = new_record.calc_sim_score(lg.event());
                    if best_match_info.is_none()
                        || current_score > best_match_info.as_ref().unwrap().2
                    {
                        best_match_info = Some((path, group_id, current_score));
                    }
                }
            }
        }

        // Mutable Phase: Update existing group or create a new one
        let root_node_for_length_mut = self
            .tree
            .entry(length)
            .or_insert_with(Node::new_internal_node);

        if let Some((best_path_to_leaf, best_group_id, best_score)) = best_match_info {
            let score_ratio = if length > 0 {
                Ratio::<BigInt>::new(BigInt::from(best_score), BigInt::from(length))
            } else {
                Ratio::<BigInt>::new(BigInt::from(0), BigInt::from(1)) // Score 0 if length is 0
            };

            if score_ratio > local_threshold {
                // Use best_path_to_leaf to get to the Vec<LogGroup>
                let target_log_groups_vec = Self::get_or_create_log_group_mut(
                    root_node_for_length_mut,
                    &best_path_to_leaf,
                    0, // Start depth from 0 for get_or_create_log_group_mut
                    local_max_depth,
                    local_max_children,
                );

                if let Some(found_group) = target_log_groups_vec
                    .iter_mut()
                    .find(|g| g.id == best_group_id)
                {
                    found_group.add_example(new_record.clone()); // Clone new_record for this case
                    return Ok(false); // Matched existing group
                }
                // If the group with best_group_id is not found, it implies an inconsistency
                // between find_candidate_log_groups/find_log_group_in_node_by_id and get_or_create_log_group_mut.
                // Fall through to create a new group, though this indicates a potential issue.
            }
        }

        // Create new group: No candidates, no root_node for length, or best match below threshold, or inconsistent state.
        let log_groups_vec_for_new = Self::get_or_create_log_group_mut(
            root_node_for_length_mut,
            &processed_line_tokens, // Path for the new group is simply its own tokens
            0,                      // Start depth from 0
            local_max_depth,
            local_max_children,
        );
        log_groups_vec_for_new.push(LogGroup::new(new_record)); // new_record is moved here
        Ok(true) // Created new group
    }

    fn collect_log_groups(&self) -> Vec<LogGroup> {
        let mut all_groups = Vec::new();
        for node in self.tree.values() {
            // Assuming collect_groups_recursive is a static/helper method or defined on Self
            // If it's an instance method, it would be self.collect_groups_recursive
            // Based on its usage elsewhere (Self::), it's likely a static helper or associated function
            // that can be called like this.
            TwoStageDrain::collect_groups_recursive(node, &mut all_groups);
        }
        all_groups
    }
}
