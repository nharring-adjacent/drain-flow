// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::{borrow::Borrow, collections::HashMap, fmt};

use anyhow::Error;
use chrono::{DateTime, Utc};
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::record::{Record, tokens::{TokenType, get_token_type}};
use crate::drains::differential_drain::TokenOrWildcard;

#[derive(Clone, Debug)]
pub struct LogGroup {
    pub id: Uuid,
    event: Record,
    examples: Vec<Record>,
    pub variables: HashMap<usize, Vec<TokenType>>, // Changed
}

// Wildcard struct removed

impl LogGroup {
    #[instrument(level = "trace", skip(event))]
    pub fn new(event: Record) -> Self {
        Self {
            id: event.uid,
            event,
            examples: vec![],
            variables: HashMap::new(),
        }
    }

    #[instrument(level = "trace", skip(self, rec))]
    pub fn add_example(&mut self, rec: Record) {
        let variable_indices = self.discover_variable_indices(&rec).unwrap_or_default();

        let mut new_vars_to_update_template = Vec::new();

        if !variable_indices.is_empty() {
            // This loop processes tokens that are different and where the template is not yet a wildcard.
            for idx in &variable_indices { // Iterate over discovered differing indices
                let current_idx = *idx; // Dereference usize

                // Type of the token in the new record 'rec' at this differing position
                let candidate_token_value = &rec.inner.inner[current_idx].1;
                let candidate_token_str = match candidate_token_value {
                    TokenOrWildcard::Token(s) => s.clone(),
                    TokenOrWildcard::Wildcard => "<*>".to_string(), // Should not happen if discover_variable_indices filters out existing wildcards
                };
                let candidate_token_type = get_token_type(&candidate_token_str);

                // Type of the token in the current template 'self.event' at this position
                let template_token_value = &self.event.inner.inner[current_idx].1;
                 let template_token_str = match template_token_value {
                    TokenOrWildcard::Token(s) => s.clone(),
                    // This path (template_token_value being Wildcard) should ideally not be taken
                    // if discover_variable_indices correctly identifies only non-wildcard template positions that differ.
                    TokenOrWildcard::Wildcard => "<*>".to_string(),
                };
                let template_token_type = get_token_type(&template_token_str);

                // Check if the candidate token type is similar to the original template token type for this position.
                // The `template_slot_types` for `is_parameter_similar` here is just the original template token's type.
                if is_parameter_similar(&candidate_token_type, &[template_token_type.clone()]) {
                    let types_at_slot = self.variables.entry(current_idx).or_insert_with(Vec::new);

                    // Add original template type (type before this generalization)
                    if !types_at_slot.contains(&template_token_type) {
                        types_at_slot.push(template_token_type.clone());
                    }
                    // Add new candidate type (the type that caused this generalization)
                    if !types_at_slot.contains(&candidate_token_type) {
                        types_at_slot.push(candidate_token_type);
                    }
                    new_vars_to_update_template.push(current_idx); // Mark for template update to <*>
                }
            }

            // Update template for positions deemed generalizable in this pass
            for idx_to_wildcard in &new_vars_to_update_template {
                 let current_idx = *idx_to_wildcard;
                 let (offset, _) = self.event.inner.inner[current_idx].clone();
                 self.event.inner.inner[current_idx] = (offset, TokenOrWildcard::Wildcard);
            }
        }

        // Second pass: iterate through all tokens of the incoming record 'rec'.
        // If a corresponding slot in the template 'self.event' is now a wildcard (either pre-existing or just made one),
        // ensure the type of the incoming token from 'rec' is registered in `self.variables`.
        for (idx, rec_token_tuple) in rec.inner.inner.iter().enumerate() {
            // Check if the current template slot at 'idx' is a Wildcard.
            if idx < self.event.inner.inner.len() && self.event.inner.inner[idx].1 == TokenOrWildcard::Wildcard {
                // Slot 'idx' in the template is indeed a wildcard.
                // Get the type of the token from the current record 'rec' at this position.
                let incoming_token_value = &rec_token_tuple.1;
                let incoming_token_str = match incoming_token_value {
                    TokenOrWildcard::Token(s) => s.clone(),
                    TokenOrWildcard::Wildcard => "<*>".to_string(), // An incoming token can itself be a wildcard string
                };
                let incoming_token_type = get_token_type(&incoming_token_str);

                let types_at_slot = self.variables.entry(idx).or_default(); // Get current types for this wildcard slot

                // If the incoming token's type is not yet in the list for this wildcard slot,
                // check if it's similar/compatible with the types already there.
                if !types_at_slot.contains(&incoming_token_type) {
                     // `is_parameter_similar` checks if `incoming_token_type` is compatible with any of `types_at_slot`.
                     // If `types_at_slot` is empty (e.g. wildcard was just made and list not populated by first loop for this specific rec's token),
                     // it should allow the type.
                    if is_parameter_similar(&incoming_token_type, types_at_slot) {
                        types_at_slot.push(incoming_token_type);
                    } else {
                        // This debug message might be too verbose if incompatibility is common.
                        // Consider if this case should result in further action or is just a silent non-addition.
                        debug!(
                            "Incoming token type {:?} at index {} for existing wildcard slot not added due to incompatibility with current slot types {:?}.",
                            incoming_token_type, idx, types_at_slot
                        );
                    }
                }
            }
        }
        self.examples.push(rec); // Add the original record as an example
    }

    #[instrument(level = "trace", skip(self))]
    pub fn event(&self) -> &Record {
        // This is the original event/base_record
        &self.event
    }

    /// Returns a reference to the base record of the log group.
    pub fn base_record(&self) -> &Record {
        &self.event
    }

    /// Returns a slice of the example records in the log group.
    pub fn examples(&self) -> &Vec<Record> {
        &self.examples
    }

    // Changed discover_variables to discover_variable_indices
    #[instrument(level = "trace", skip(self, rec))]
    pub fn discover_variable_indices(&self, rec: &Record) -> Result<Vec<usize>, Error> {
        let indices = self
            .event
            .borrow()
            .into_iter() // Iterates over &TokenOrWildcard from the template (self.event)
            .enumerate()
            .zip(rec.into_iter()) // Iterates over &TokenOrWildcard from the candidate record 'rec' (since rec is &Record)
            .filter_map(|((idx, template_token_ref), candidate_token_ref)| {
                // template_token_ref is &TokenOrWildcard
                // candidate_token_ref is &TokenOrWildcard

                // We are looking for positions to potentially turn into new wildcards.
                // Such a position must not already be a wildcard in the template.
                if matches!(template_token_ref, TokenOrWildcard::Wildcard) {
                    return None; // Template slot is already a wildcard, not a candidate for *new* wildcarding.
                }

                // Compare the template's TokenOrWildcard with the candidate's TokenOrWildcard.
                // Both are references, so dereference or allow PartialEq to handle &T == &U if implemented.
                // Direct comparison `template_token_ref != candidate_token_ref` works if PartialEq<&TokenOrWildcard> for &TokenOrWildcard is fine.
                // Or compare owned values: `*template_token_ref != *candidate_token_ref`
                if template_token_ref != candidate_token_ref {
                    debug!(idx, ?template_token_ref, ?candidate_token_ref, "found candidate for new wildcard based on TokenOrWildcard difference");
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        Ok(indices)
    }

    // update_variables removed, logic integrated into add_example

    /// Number of examples this [LogGroup] contains
    #[instrument(level = "trace", skip_all)]
    pub fn len(&self) -> usize {
        self.examples.len()
    }

    /// Whether any examples exist for a [LogGroup]
    #[instrument(level = "trace", skip_all)]
    pub fn is_empty(&self) -> bool {
        self.examples.is_empty()
    }

    /// Return a Vec<&Record> of the example records for this group
    #[instrument(level = "trace", skip_all)]
    pub fn get_examples(&self) -> Vec<&Record> {
        self.examples.iter().collect::<Vec<&Record>>()
    }

    /// Returns the [Uuid] associated with the [LogGroup], usually identical to the [Record] which created the group
    #[instrument(level = "trace", skip_all)]
    pub fn get_id(&self) -> Uuid {
        self.id
    }

    /// Returns the [DateTime] of the creation of the base event in the [LogGroup]
    #[instrument(level = "trace", skip_all)]
    pub fn get_time(&self) -> DateTime<Utc> {
        self.event.uid.get_timestamp().map_or(Utc::now(), |ts| {
            let (secs_u64, nanos) = ts.to_unix();
            let secs_i64 = secs_u64 as i64;
            DateTime::from_timestamp(secs_i64, nanos).unwrap_or_else(Utc::now)
        })
    }
}

fn are_types_compatible_for_generalization(type1: &TokenType, type2: &TokenType) -> bool {
    // Basic rule: if types are the same, they are compatible.
    if type1 == type2 {
        return true;
    }

    // Numeric types can be generalized together.
    match (type1, type2) {
        (TokenType::Integer, TokenType::Float) | (TokenType::Float, TokenType::Integer) => return true,
        // Network types might be considered compatible for a generic wildcard
        (TokenType::IpAddress, TokenType::Hostname) | (TokenType::Hostname, TokenType::IpAddress) => return true,
        // Allow general words to absorb more specific but Word-like types if needed, or vice-versa
        (TokenType::Word, TokenType::Hostname) | (TokenType::Hostname, TokenType::Word) => true,
        (TokenType::Word, TokenType::BooleanStatus) | (TokenType::BooleanStatus, TokenType::Word) => true,
        (TokenType::FilePath, TokenType::UrlPath) | (TokenType::UrlPath, TokenType::FilePath) => true,
        (TokenType::AlphanumericId, TokenType::Integer) | (TokenType::Integer, TokenType::AlphanumericId) => true,
        (TokenType::AlphanumericId, TokenType::Word) | (TokenType::Word, TokenType::AlphanumericId) => true,
        (TokenType::AlphanumericId, TokenType::Hostname) | (TokenType::Hostname, TokenType::AlphanumericId) => true,
        // Wildcard type can be generalized with any other type.
        // This means if a slot already contains Wildcard due to <*>, it can accept any new type.
        // Or if new type is Wildcard (e.g. from a literal <*> token), it's compatible.
        (TokenType::Wildcard, _) | (_, TokenType::Wildcard) => true,
        // Other can be generalized with many things, acting as a fallback.
        // Consider if Other should be more restrictive. For now, it's quite permissive.
        (TokenType::Other, TokenType::Word) | (TokenType::Word, TokenType::Other) => true,
        (TokenType::Other, TokenType::AlphanumericId) | (TokenType::AlphanumericId, TokenType::Other) => true,
        (TokenType::Other, TokenType::Integer) | (TokenType::Integer, TokenType::Other) => true, // e.g. "item1" vs "123"
        (TokenType::Other, TokenType::Float) | (TokenType::Float, TokenType::Other) => true,
        _ => false, // Default to not compatible
    }
}

pub fn is_parameter_similar(
    new_token_candidate_type: &TokenType, // Type of the token from the new log line
    template_slot_types: &[TokenType], // Types already seen in this wildcard slot OR the single type of the original template token.
) -> bool {
    // If the template_slot_types is empty, it means we are trying to add a type to an empty list of types for a slot.
    // This typically happens if a slot was just made a wildcard and we are adding the first type(s) to it,
    // or in the second loop of add_example, if a slot was wildcarded but its types list is somehow still empty.
    // In this scenario, any new type is considered "similar enough" to start populating the list.
    if template_slot_types.is_empty() {
        return true;
    }

    // If the new token's type is exactly one of the types already in the slot, it's similar.
    if template_slot_types.contains(new_token_candidate_type) {
        return true;
    }

    // If the slot is not empty, check if the new type is compatible with ANY of the existing types in the slot.
    // This is the core logic: can the new type be generalized with what's already there?
    for existing_type in template_slot_types {
        if are_types_compatible_for_generalization(new_token_candidate_type, existing_type) {
            return true;
        }
    }

    // If the new type is not present and not compatible with any existing type in the slot,
    // then it's not similar for generalization.
    false
}


impl fmt::Display for LogGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LogGroup ID: {}\nFirst Seen: {}\nEvent: {}\n{} examples and {} variable slots\n", // Changed wildcards to variable slots
            self.event.uid,
            self.get_time(),
            self.event,
            self.examples.len(),
            self.variables.len()
        )
    }
}

#[cfg(test)]
mod should {
    use spectral::prelude::*;

    // Wildcard struct removed
    use crate::{
        drains::differential_drain::TokenOrWildcard, // Added TokenOrWildcard
        log_group::LogGroup,
        record::{Record, tokens::TokenType}, // Removed tokens::Token
    };

    #[test]
    fn test_discover_variable_indices() { // Renamed test and adjusted assertions
        let rec1 = Record::new("Common prefix Common prefix Common prefix 1234".to_string());
        let lg = LogGroup::new(rec1);
        let rec2 = Record::new("Common prefix Common prefix Common prefix 3456".to_string());
        let indices = lg.discover_variable_indices(&rec2);
        assert_that(&indices).is_ok_containing(vec![6]); // Asserting index
    }

    #[test]
    fn test_add_example_generalizes_and_stores_types() { // Replaces test_update_variables
        let r1 = Record::new("ID user123 connected from 192.168.1.1".to_string());
        let mut lg = LogGroup::new(r1);

        let r2 = Record::new("ID user456 connected from 10.0.0.1".to_string());
        lg.add_example(r2);

        // Check that index 1 (user123/user456) became a wildcard
        assert_that(&lg.event.inner.inner[1].1).is_equal_to(TokenOrWildcard::Wildcard);
        // Check that index 5 (IP Addresses) became a wildcard
        assert_that(&lg.event.inner.inner[5].1).is_equal_to(TokenOrWildcard::Wildcard);

        // Check stored types for index 1
        assert_that(&lg.variables.get(&1)).is_some();
        let types_idx1 = lg.variables.get(&1).unwrap();
        // Assuming AlphanumericId for user123 and user456 based on current regex
        assert_that(types_idx1).contains(TokenType::AlphanumericId);

        // Check stored types for index 5
        assert_that(&lg.variables.get(&5)).is_some();
        let types_idx5 = lg.variables.get(&5).unwrap();
        assert_that(types_idx5).contains(TokenType::IpAddress);

        // Add another example with a hostname to test generalization
        let r3 = Record::new("ID user789 connected from server.example.com".to_string());
        lg.add_example(r3);

        assert_that(&lg.event.inner.inner[5].1).is_equal_to(TokenOrWildcard::Wildcard); // Should still be wildcard
        let types_idx5_updated = lg.variables.get(&5).unwrap();
        assert_that(types_idx5_updated).contains(TokenType::IpAddress);
        assert_that(types_idx5_updated).contains(TokenType::Hostname); // Now also Hostname

        // Example where a new type is not compatible and should not make it a wildcard (or change existing)
        let r4 = Record::new("ID user000 connected from /var/log/sys.log".to_string()); // FilePath, not compatible with IP/Hostname
        lg.add_example(r4);

        // Slot 5 should still be a wildcard, but FilePath should not be added to its types
        // because is_parameter_similar should prevent it based on current rules.
        // The event itself for slot 5 will remain Wildcard because it was already made so.
        // The question is whether FilePath is added to variables[5]
        let types_idx5_after_incompatible = lg.variables.get(&5).unwrap();
        assert_that(types_idx5_after_incompatible).contains(TokenType::IpAddress);
        assert_that(types_idx5_after_incompatible).contains(TokenType::Hostname);
        assert_that(types_idx5_after_incompatible).does_not_contain(TokenType::FilePath);


        // Test with an integer and then a float
        let r_int = Record::new("Value is 100 units".to_string());
        let mut lg_numeric = LogGroup::new(r_int);
        let r_float = Record::new("Value is 200.5 units".to_string());
        lg_numeric.add_example(r_float);

        assert_that(&lg_numeric.event.inner.inner[2].1).is_equal_to(TokenOrWildcard::Wildcard);
        let types_numeric = lg_numeric.variables.get(&2).unwrap();
        assert_that(types_numeric).contains(TokenType::Integer);
        assert_that(types_numeric).contains(TokenType::Float);
    }
}
