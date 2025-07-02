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

/// Represents the kind of a node in the `TwoStageDrain`'s tree structure.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// A leaf node, containing a vector of `LogGroup`s.
    Leaf(Vec<LogGroup>),
    /// An internal node, containing a map of child nodes keyed by `DefaultSymbol`.
    Internal(HashMap<DefaultSymbol, Node>),
}

/// Represents a node in the `TwoStageDrain`'s tree structure.
///
/// Each node has a unique ID and can either be a `Leaf` node (containing log groups)
/// or an `Internal` node (containing child nodes).
#[derive(Debug, Clone)]
pub struct Node {
    /// The unique identifier for this node.
    pub id: Uuid,
    /// The kind of node, either `Leaf` or `Internal`.
    pub kind: NodeKind,
}

impl Node {
    /// Creates a new internal node with a randomly generated UUID and an empty map of children.
    ///
    /// # Returns
    ///
    /// A new `Node` instance of `NodeKind::Internal`.
    pub fn new_internal_node() -> Self {
        Node {
            id: Uuid::new_v4(),
            kind: NodeKind::Internal(HashMap::new()),
        }
    }

    /// Creates a new leaf node with a randomly generated UUID and an empty vector of log groups.
    ///
    /// # Returns
    ///
    /// A new `Node` instance of `NodeKind::Leaf`.
    pub fn new_leaf_node() -> Self {
        Node {
            id: Uuid::new_v4(),
            kind: NodeKind::Leaf(Vec::new()),
        }
    }

    /// Creates a new leaf node with a randomly generated UUID and a pre-existing vector of log groups.
    ///
    /// # Arguments
    ///
    /// * `groups` - A `Vec<LogGroup>` to initialize the leaf node with.
    ///
    /// # Returns
    ///
    /// A new `Node` instance of `NodeKind::Leaf` containing the provided groups.
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
        assert_that(&Drain::process_line(
            &mut drain,
            "This is a test log line".to_string(),
        ))
        .is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_process_line_matches_existing_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 10, 10).unwrap(); // Increased max_depth for this test
        let _ = Drain::process_line(&mut drain, "Log message type A value1".to_string());
        assert_that(&Drain::process_line(
            &mut drain,
            "Log message type A value2".to_string(),
        ))
        .is_ok_containing(false);
    }

    #[traced_test]
    #[test]
    fn test_process_line_creates_second_group() {
        let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10).unwrap();
        let _ = Drain::process_line(&mut drain, "Log message type A value1".to_string());
        assert_that(&Drain::process_line(
            &mut drain,
            "Completely different log message valueX".to_string(),
        ))
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

/// Recursively collects all `LogGroup`s from a given node and its subtree.
///
/// This is a helper function used during tree traversal to gather all log groups
/// that are candidates for matching a new log line.
///
/// # Arguments
///
/// * `node` - The current node to start collecting log groups from.
/// * `path_to_node` - The path (sequence of `DefaultSymbol`s) from the root of the
///   length-specific tree to the `node`.
/// * `candidate_paths` - A mutable vector to accumulate tuples of `(path_to_leaf, log_group_id)`.
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

/// `TwoStageDrain` is an advanced implementation of the `Drain` trait that uses a
/// tree-like structure to efficiently cluster log messages. It employs a two-stage
/// process: first, preprocessing log lines to replace domain-specific patterns with
/// wildcards, and second, organizing and matching these processed lines within a
/// hierarchical tree based on their token sequences.
#[derive(Debug, Clone)]
pub struct TwoStageDrain {
    /// Regular expressions defining the domain patterns to be replaced in log lines.
    pub domain: Vec<Regex>,
    /// The root of the tree structure, where keys are log line lengths and values are the
    /// corresponding root nodes for that length.
    pub tree: HashMap<usize, Node>,
    /// The similarity threshold used to determine if a new log line matches an existing
    /// `LogGroup` within the tree.
    pub threshold: Ratio<BigInt>,
    /// A shared string interner for efficient storage and comparison of log tokens.
    pub strings: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>>,
    /// The maximum depth of the tree, which influences the specificity of log templates.
    pub max_depth: usize,
    /// The maximum number of children an internal node can have before it is converted
    /// into a leaf node (generalizing its subtree).
    pub max_children: usize,
    /// A counter for the total number of log lines processed by this drain.
    pub line_count_processed: usize,
}

impl TwoStageDrain {
    /// Recursively finds candidate `LogGroup`s within the tree that a new log record
    /// might match. This function traverses the tree based on the tokens of the
    /// incoming log record, considering both specific token matches and wildcard matches.
    ///
    /// # Arguments
    ///
    /// * `current_node` - The current node being examined in the tree traversal.
    /// * `record_tokens` - The tokenized representation of the log record being processed.
    /// * `current_path` - The path (sequence of symbols) from the root of the length-specific
    ///   tree to the `current_node`.
    /// * `current_depth` - The current depth in the tree traversal.
    /// * `max_depth` - The maximum allowed depth for the tree.
    /// * `candidate_paths` - A mutable vector to collect the paths and IDs of potential
    ///   matching `LogGroup`s.
    #[allow(clippy::only_used_in_recursion)]
    fn find_candidate_log_groups(
        &self,
        current_node: &Node,
        record_tokens: &[DefaultSymbol],
        current_path: Vec<DefaultSymbol>, 
        current_depth: usize,
        max_depth: usize,
        candidate_paths: &mut Vec<(Vec<DefaultSymbol>, Uuid)>, 
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

    /// Finds a `LogGroup` by its ID within a given node's subtree.
    ///
    /// This is a recursive helper function for immutable searching.
    ///
    /// # Arguments
    ///
    /// * `node` - The node to start the search from.
    /// * `group_id` - The `Uuid` of the `LogGroup` to find.
    ///
    /// # Returns
    ///
    /// An `Option<&LogGroup>` containing a reference to the found `LogGroup` if it exists,
    /// otherwise `None`.
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

    /// Collects all `LogGroup`s from a given node and its subtree, consuming the nodes.
    ///
    /// This function is used when restructuring the tree, for example, when converting
    /// an internal node to a leaf node. It moves the `LogGroup`s out of the nodes.
    ///
    /// # Arguments
    ///
    /// * `node` - The node to collect log groups from (mutable).
    /// * `all_log_groups` - A mutable vector to append the collected `LogGroup`s to.
    fn collect_log_groups_from_node(node: &mut Node, all_log_groups: &mut Vec<LogGroup>) {
        match node.kind {
            NodeKind::Leaf(ref mut groups) => {
                all_log_groups.append(&mut std::mem::take(groups));
            }
            NodeKind::Internal(ref mut children_map) => {
                for child_node in children_map.values_mut() {
                    Self::collect_log_groups_from_node(child_node, all_log_groups);
                }
            }
        }
    }

    /// Recursively collects all `LogGroup`s from a given node and its subtree.
    ///
    /// This is a read-only traversal used for collecting all log groups for external
    /// consumption (e.g., by the `collect_log_groups` method of the `Drain` trait).
    ///
    /// # Arguments
    ///
    /// * `node` - The node to start collecting log groups from.
    /// * `collected_groups` - A mutable vector to append the cloned `LogGroup`s to.
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

    /// Gets a mutable reference to the `Vec<LogGroup>` where a new log group should be
    /// inserted or an existing one updated. This function traverses or creates nodes
    /// in the tree as necessary to reach the appropriate leaf node.
    ///
    /// # Arguments
    ///
    /// * `current_node` - The current node in the tree traversal (mutable).
    /// * `record_tokens` - The tokenized representation of the log record.
    /// * `current_depth` - The current depth in the tree traversal.
    /// * `max_depth` - The maximum allowed depth for the tree.
    /// * `max_children` - The maximum number of children an internal node can have.
    ///
    /// # Returns
    ///
    /// A mutable reference to the `Vec<LogGroup>` at the target leaf node.
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

    /// Creates a new `TwoStageDrain` instance.
    ///
    /// # Arguments
    ///
    /// * `domain_regex_strings` - A vector of strings, each representing a regular
    ///   expression pattern to be used for preprocessing log lines. These patterns
    ///   are replaced with a wildcard token (`<*>`) before further processing.
    /// * `threshold` - The similarity threshold (a float between 0.0 and 1.0) used
    ///   to determine if a new log line matches an existing `LogGroup`.
    /// * `max_depth` - The maximum depth of the tree. This controls how specific
    ///   log templates can be. A higher depth allows for more specific patterns.
    /// * `max_children` - The maximum number of children an internal node can have.
    ///   If a node exceeds this limit, it is converted into a leaf node, and its
    ///   subtree is generalized.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `TwoStageDrain` instance on success, or an
    /// `anyhow::Error` if any of the provided domain patterns are invalid regular
    /// expressions or if the threshold value is invalid.
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
    /// Processes a single log line, attempting to match it to an existing log group
    /// within the tree structure. If a suitable match is found, the log line is
    /// added to that group. Otherwise, a new log group is created and inserted
    /// into the tree.
    ///
    /// The process involves:
    /// 1. Preprocessing the log line by replacing domain-specific patterns with wildcards.
    /// 2. Tokenizing the preprocessed line.
    /// 3. Traversing the tree to find candidate log groups based on the token sequence.
    /// 4. Calculating similarity scores and selecting the best match.
    /// 5. Updating the matched log group or creating a new one.
    ///
    /// # Arguments
    ///
    /// * `line` - The log line to be processed.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if a new log group was created.
    /// * `Ok(false)` if the log line was added to an existing log group.
    /// * `Err(anyhow::Error)` if an error occurred during processing.
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

    /// Collects all `LogGroup`s currently stored in the drain.
    ///
    /// This method traverses the tree structure and gathers all log groups
    /// from leaf nodes.
    ///
    /// # Returns
    ///
    /// A `Vec<LogGroup>` containing clones of all log groups.
    fn collect_log_groups(&self) -> Vec<LogGroup> {
        let mut all_groups = Vec::new();
        for node in self.tree.values() {
            TwoStageDrain::collect_groups_recursive(node, &mut all_groups);
        }
        all_groups
    }
}
