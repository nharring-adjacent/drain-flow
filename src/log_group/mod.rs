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

use crate::record::{tokens::Token, Record};

/// Represents a logical grouping of similar log records.
///
/// A `LogGroup` is characterized by a base event (a `Record`) and a collection
/// of example records that match the group's pattern. It also tracks variables
/// (wildcards) within the log pattern.
#[derive(Clone, Debug)]
pub struct LogGroup {
    /// The unique identifier for this log group.
    pub id: Uuid,
    /// The base event or representative record for this log group.
    event: Record,
    /// A collection of log records that belong to this group.
    examples: Vec<Record>,
    /// A map of variable positions (offset) to their `Token` type within the event pattern.
    pub variables: HashMap<usize, Token>,
}

/// Represents a wildcard (variable) found within a log pattern.
///
/// It stores the offset (position) of the wildcard within the log line
/// and the `Token` type of the wildcard.
#[derive(Clone, Debug, PartialEq)]
pub struct Wildcard((usize, Token));

impl fmt::Display for Wildcard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0 .0)
    }
}

impl LogGroup {
    /// Creates a new `LogGroup` from an initial `Record`.
    ///
    /// The provided `event` becomes the base record for the group, and is also
    /// added as the first example.
    ///
    /// # Arguments
    ///
    /// * `event` - The initial `Record` that defines this log group.
    ///
    /// # Returns
    ///
    /// A new `LogGroup` instance.
    #[instrument(level = "trace", skip(event))]
    pub fn new(event: Record) -> Self {
        let id = event.uid;
        Self {
            id,
            examples: vec![event.clone()],
            event,
            variables: HashMap::new(),
        }
    }

    /// Adds a new example `Record` to the log group.
    ///
    /// This method also attempts to discover new variables (wildcards) by comparing
    /// the new record with the group's base event and updates the group's variable map.
    ///
    /// # Arguments
    ///
    /// * `rec` - The `Record` to add as an example.
    #[instrument(level = "trace", skip(self, rec))]
    pub fn add_example(&mut self, rec: Record) {
        let vars = self.discover_variables(&rec).unwrap();
        self.examples.push(rec);
        if !vars.is_empty() {
            self.update_variables(vars);
        }
    }

    /// Returns a reference to the base event (`Record`) of this log group.
    ///
    /// This record represents the generalized pattern of the log group.
    ///
    /// # Returns
    ///
    /// A reference to the `Record` that is the base event.
    #[instrument(level = "trace", skip(self))]
    pub fn event(&self) -> &Record {
        // This is the original event/base_record
        &self.event
    }

    /// Returns a reference to the base record of the log group.
    ///
    /// This is an alias for `event()`.
    ///
    /// # Returns
    ///
    /// A reference to the `Record` that is the base record.
    pub fn base_record(&self) -> &Record {
        &self.event
    }

    /// Returns a slice of the example records stored in this log group.
    ///
    /// These are the actual log lines that have been clustered into this group.
    ///
    /// # Returns
    ///
    /// A slice (`&Vec<Record>`) of the example records.
    pub fn examples(&self) -> &Vec<Record> {
        &self.examples
    }

    /// Compares a given `Record` with the log group's base event to identify variable positions.
    ///
    /// Positions where the tokens differ between the record and the base event,
    /// and are not already identified as variables, are considered new variables.
    ///
    /// # Arguments
    ///
    /// * `rec` - The `Record` to compare against the base event.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `Vec<Wildcard>` representing the newly discovered
    /// variable positions, or an `anyhow::Error` if the comparison fails.
    #[instrument(level = "trace", skip(self, rec))]
    pub fn discover_variables(&self, rec: &Record) -> Result<Vec<Wildcard>, Error> {
        let f = self
            .event
            .borrow()
            .into_iter()
            .enumerate()
            .zip(rec.into_iter())
            .filter(|((idx, event), candidate)| {
                if self.variables.contains_key(idx) {
                    // This token has already been identified as a variable
                    false
                } else if event != candidate {
                    debug!(%idx, ?event, ?candidate, "found candidate");
                    true
                } else {
                    false
                }
            })
            .map(|((idx, _event), _candidate)| Wildcard((idx, Token::Wildcard)))
            .collect::<Vec<_>>();
        Ok(f)
    }

    /// Updates the log group's variable map and base event with newly discovered wildcards.
    ///
    /// This method is typically called after `discover_variables` to incorporate
    /// the identified variables into the group's pattern.
    ///
    /// # Arguments
    ///
    /// * `vars` - A `Vec<Wildcard>` containing the variables to update.
    #[instrument(level = "trace", skip(self, vars))]
    fn update_variables(&mut self, vars: Vec<Wildcard>) {
        for var in vars {
            // Assume we got vars from discover_variables so it has already checked against this map
            self.variables.insert(var.0 .0, var.0 .1.clone());
            // Update the tokens in the base event as well
            let (offset, _) = self.event.inner.inner[var.0 .0].clone();
            self.event.inner.inner[var.0 .0] = (offset, var.0 .1);
        }
    }

    /// Returns the total number of example records stored in this `LogGroup`.
    ///
    /// # Returns
    ///
    /// The number of examples as a `usize`.
    #[instrument(level = "trace", skip_all)]
    pub fn len(&self) -> usize {
        self.examples.len()
    }

    /// Checks if the log group contains any example records.
    ///
    /// # Returns
    ///
    /// `true` if the log group has no examples, `false` otherwise.
    #[instrument(level = "trace", skip_all)]
    pub fn is_empty(&self) -> bool {
        self.examples.is_empty()
    }

    /// Returns a vector of references to the example records for this group.
    ///
    /// # Returns
    ///
    /// A `Vec<&Record>` containing references to all example records.
    #[instrument(level = "trace", skip_all)]
    pub fn get_examples(&self) -> Vec<&Record> {
        self.examples.iter().collect::<Vec<&Record>>()
    }

    /// Returns the unique identifier (`Uuid`) associated with this `LogGroup`.
    ///
    /// This ID is typically the same as the `Uuid` of the `Record` that created the group.
    ///
    /// # Returns
    ///
    /// The `Uuid` of the log group.
    #[instrument(level = "trace", skip_all)]
    pub fn get_id(&self) -> Uuid {
        self.id
    }

    /// Returns the creation timestamp of the base event in the `LogGroup` as a `DateTime<Utc>`.
    ///
    /// This timestamp is derived from the `Uuid` of the base event.
    ///
    /// # Returns
    ///
    /// A `DateTime<Utc>` representing the creation time of the base event.
    #[instrument(level = "trace", skip_all)]
    pub fn get_time(&self) -> DateTime<Utc> {
        // Uuid::get_timestamp returns Option<Timestamp>
        // Timestamp::to_unix returns (i64, u32)
        // Ksuid::get_time returns DateTime<Utc>
        // For now, let's assume we want to keep the DateTime<Utc> type
        // This will require more significant changes if we need to extract time directly from Uuid
        // Uuid::get_timestamp returns Option<Timestamp>
        // Timestamp::to_unix returns (u64, u32) for UUIDv1
        // DateTime::from_timestamp expects i64 for seconds.
        self.event.uid.get_timestamp().map_or(Utc::now(), |ts| {
            let (secs_u64, nanos) = ts.to_unix();
            // Convert u64 seconds to i64. This is safe as long as the timestamp is not
            // extremely far in the future, which is a reasonable assumption for log events.
            let secs_i64 = secs_u64 as i64;
            DateTime::from_timestamp(secs_i64, nanos).unwrap_or_else(Utc::now)
        })
    }
}

impl fmt::Display for LogGroup {
    /// Formats the `LogGroup` for display.
    ///
    /// This implementation provides a human-readable summary of the log group,
    /// including its ID, first seen timestamp, base event, number of examples,
    /// and number of wildcards.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write into.
    ///
    /// # Returns
    ///
    /// A `fmt::Result` indicating success or failure of the formatting operation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LogGroup ID: {}\nFirst Seen: {}\nEvent: {}\n{} examples and {} wildcards\n",
            self.event.uid,  // Changed from serialize()
            self.get_time(), // Changed from self.event.uid.get_time() to use the struct's method
            self.event,
            self.examples.len(),
            self.variables.len()
        )
    }
}

#[cfg(test)]
mod should {
    use spectral::prelude::*;

    use super::Wildcard;
    use crate::{
        log_group::LogGroup,
        record::{tokens::Token, Record},
    };

    #[test]
    fn test_discover_variables() {
        let rec1 = Record::new("Common prefix Common prefix Common prefix 1234".to_string());
        let lg = LogGroup::new(rec1);
        let rec2 = Record::new("Common prefix Common prefix Common prefix 3456".to_string());
        let vars = lg.discover_variables(&rec2);
        assert_that(&vars).is_ok_containing(vec![Wildcard((6, Token::Wildcard))]);
    }

    #[test]
    fn test_update_variables() {
        let r1 = Record::new("Common Prefix Common Prefix Common Prefix 6789".to_string());
        let r2 = Record::new("Common Prefix Common Prefix Common Prefix 827364".to_string());
        let mut lg = LogGroup::new(r1);

        let vars = lg.discover_variables(&r2).unwrap();
        lg.update_variables(vars);
        assert_that(&lg.variables).contains_key(6);
    }
}
