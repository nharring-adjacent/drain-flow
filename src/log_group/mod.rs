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
use rksuid::Ksuid;
use tracing::{debug, instrument};

use crate::record::{tokens::Token, Record};

#[derive(Clone, Debug)]
pub struct LogGroup {
    pub id: Ksuid,
    event: Record,
    examples: Vec<Record>,
    pub variables: HashMap<usize, Token>,
}

/// A wildcard is an offset and a typed token
#[derive(Clone, Debug, PartialEq)]
pub struct Wildcard((usize, Token));

impl fmt::Display for Wildcard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0 .0)
    }
}

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
        let vars = self.discover_variables(&rec).unwrap();
        self.examples.push(rec);
        if !vars.is_empty() {
            self.update_variables(vars);
        }
    }

    #[instrument(level = "trace", skip(self))]
    pub fn event(&self) -> &Record {
        &self.event
    }

    /// Compare a record with this log group and identify positions which qualify as variables, returned as vector of [Wildcard]
    #[instrument(level = "trace", skip(self, rec))]
    pub fn discover_variables(&self, rec: &Record) -> Result<Vec<Wildcard>, Error> {
        let f = self
            .event
            .borrow()
            .into_iter()
            .enumerate()
            .zip(rec.into_iter())
            .filter(|((idx, event), candidate)| {
                if self.variables.get(idx).is_some() {
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

    /// Returns the [Ksuid] associated with the [LogGroup], usually identical to the [Record] which created the group
    #[instrument(level = "trace", skip_all)]
    pub fn get_id(&self) -> Ksuid {
        self.id
    }

    /// Returns the [DateTime] of the creation of the base event in the [LogGroup]
    #[instrument(level = "trace", skip_all)]
    pub fn get_time(&self) -> DateTime<Utc> {
        self.event.uid.get_time()
    }
}

impl fmt::Display for LogGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LogGroup ID: {}\nFirst Seen: {}\nEvent: {}\n{} examples and {} wildcards\n",
            self.event.uid.serialize(),
            self.event.uid.get_time(),
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
