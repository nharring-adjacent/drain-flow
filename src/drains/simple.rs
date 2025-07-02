// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::{collections::HashMap, fmt, sync::Arc};

use anyhow::{anyhow, Error};
use fraction::{BigInt, FromPrimitive, Ratio};
use joinery::{Joinable, JoinableIterator};
use lazy_static::lazy_static;
use parking_lot::RwLock;
use regex::Regex;
use string_interner::{DefaultSymbol, StringInterner};
use tracing::instrument;

use crate::{drains::api::Drain, log_group::LogGroup, record::Record};

lazy_static! {
    pub(crate) static ref INTERNER: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>> =
        Arc::new(RwLock::new(StringInterner::<
            string_interner::backend::BucketBackend,
        >::new()));
}
#[derive(Debug, Clone)]
pub struct SingleLayer {
    /// Regular expressions defining the domain patterns to be replaced in log lines.
    pub domain: Vec<Regex>,
    /// The core storage for log groups, organized by log line length and first token.
    base_layer: HashMap<usize, HashMap<DefaultSymbol, Vec<LogGroup>>>,
    /// The similarity threshold used to determine if a new log line matches an existing `LogGroup`.
    pub threshold: Ratio<BigInt>,
    /// A shared string interner for efficient storage and comparison of log tokens.
    strings: Arc<RwLock<StringInterner<string_interner::backend::BucketBackend>>>,
}

impl SingleLayer {
    /// Creates a new `SingleLayer` drain instance.
    ///
    /// # Arguments
    ///
    /// * `domain` - A vector of strings, each representing a regular expression
    ///   pattern to be used for preprocessing log lines. These patterns are replaced
    ///   with a wildcard token before further processing.
    ///
    /// # Returns
    ///
    /// A `Result` containing the new `SingleLayer` instance on success, or an
    /// `anyhow::Error` if any of the provided domain patterns are invalid regular expressions.
    #[instrument(skip(domain))]
    pub fn new(domain: Vec<String>) -> Result<Self, Error> {
        let patterns = domain
            .iter()
            .map(|s| Regex::new(s))
            .collect::<Result<Vec<Regex>, regex::Error>>()?;
        Ok(Self {
            domain: patterns,
            base_layer: HashMap::new(),
            threshold: Ratio::from_float::<f32>(0.5).expect("0.5 converts into a ratio"),
            strings: INTERNER.clone(),
        })
    }

    /// Sets the similarity threshold for the drain.
    ///
    /// This threshold determines how similar a new log line must be to an existing
    /// `LogGroup`'s event template to be considered a match. The similarity is
    /// calculated as a ratio of matching tokens to total tokens.
    ///
    /// # Arguments
    ///
    /// * `numerator` - The numerator of the similarity ratio.
    /// * `denominator` - The denominator of the similarity ratio.
    ///
    /// # Returns
    ///
    /// `Ok(())` on success, or an `anyhow::Error` if the numerator or denominator
    /// cannot be converted into `BigInt`.
    #[instrument(skip(self))]
    pub fn set_threshold(&mut self, numerator: u64, denominator: u64) -> Result<(), Error> {
        let numer = BigInt::from_u64(numerator)
            .ok_or_else(|| anyhow!("unable to make numerator from {}", numerator))?;
        let denom = BigInt::from_u64(denominator)
            .ok_or_else(|| anyhow!("unable to make denominator from {}", denominator))?;
        let new_ratio = Ratio::new(numer, denom);
        self.threshold = new_ratio;
        Ok(())
    }

    /// Iterates over all `LogGroup`s stored within the drain.
    ///
    /// This method provides a flattened view of all log groups, regardless of their
    /// internal organization (by length and first token).
    ///
    /// # Returns
    ///
    /// A `Vec<Vec<&LogGroup>>` containing references to all log groups.
    #[instrument(skip(self), level = "trace")]
    fn iter_groups(&self) -> Vec<Vec<&LogGroup>> {
        let mut results: Vec<Vec<&LogGroup>> = Vec::new();
        for length in self.base_layer.keys() {
            let mut groups = vec![];
            for (_, grp) in self.base_layer.get(length).unwrap().iter() {
                for g in grp {
                    groups.push(g);
                }
            }
            results.push(groups);
        }
        results
    }

    /// Resolves a `DefaultSymbol` back into its original string representation.
    ///
    /// This is a utility method that uses the internal string interner to retrieve
    /// the string associated with a given symbol.
    ///
    /// # Arguments
    ///
    /// * `sym` - The `DefaultSymbol` to resolve.
    ///
    /// # Returns
    ///
    /// A `String` representation of the symbol.
    ///
    /// # Panics
    ///
    /// Panics if the symbol cannot be resolved, which should not happen under normal
    /// operation as symbols are only created from interned strings.
    #[instrument(skip(self), level = "trace")]
    pub fn resolve(&self, sym: DefaultSymbol) -> String {
        self.strings
            .read()
            .resolve(sym)
            .expect("symbols must resolve")
            .to_owned()
    }
}

impl Drain for SingleLayer {
    /// Processes a single log line, attempting to match it against existing log groups.
    ///
    /// If the line is empty, it is ignored. Otherwise, it is converted into a `Record`.
    /// The method then attempts to find a matching `LogGroup` based on the record's
    /// length and first token. If a sufficiently similar `LogGroup` is found (based on
    /// the `threshold`), the new record is added as an example to that group. If no
    /// match is found, a new `LogGroup` is created for the record.
    ///
    /// # Arguments
    ///
    /// * `line` - The log line string to process.
    ///
    /// # Returns
    ///
    /// * `Ok(true)` if a new `LogGroup` was created.
    /// * `Ok(false)` if the line was added to an existing `LogGroup`.
    /// * `Err(anyhow::Error)` if an error occurred during processing.
    #[instrument(skip(self, line))]
    fn process_line(&mut self, line: String) -> Result<bool, Error> {
        if line.is_empty() {
            return Ok(false);
        }
        let new_record = Record::new(line);
        let length = new_record.len();
        let first = new_record.first().expect("records have first tokens");
        if let Some(second_layer) = self.base_layer.get_mut(&length) {
            match second_layer.get_mut(&first) {
                Some(log_groups) => {
                    let (score, offset) = log_groups.iter_mut().enumerate().fold(
                        (
                            0, // best score
                            0, // index of best score LogGroup
                        ),
                        |mut acc, elem| {
                            let score = new_record.clone().calc_sim_score(elem.1.event());
                            if score > acc.0 {
                                acc = (score, elem.0); // overwrite state with new values
                            }
                            acc
                        },
                    );
                    let score_ratio =
                        Ratio::<BigInt>::new(BigInt::from(score), BigInt::from(length));
                    if score_ratio > self.threshold {
                        // add this record's uid to the list of examples for the log group
                        log_groups[offset].add_example(new_record);
                        Ok(false)
                    } else {
                        log_groups.push(LogGroup::new(new_record));
                        Ok(true)
                    }
                }
                None => {
                    second_layer.insert(first, vec![LogGroup::new(new_record)]);
                    Ok(true)
                }
            }
        } else {
            self.base_layer.insert(length, HashMap::new());
            let second_layer = self
                .base_layer
                .get_mut(&length)
                .expect("We just inserted this map");
            second_layer.insert(first, vec![LogGroup::new(new_record)]);
            Ok(true)
        }
    }

    /// Collects all `LogGroup`s currently stored in the drain.
    ///
    /// This method flattens the internal hierarchical storage of log groups
    /// into a single vector.
    ///
    /// # Returns
    ///
    /// A `Vec<LogGroup>` containing clones of all log groups.
    fn collect_log_groups(&self) -> Vec<LogGroup> {
        self.iter_groups().into_iter().flatten().cloned().collect()
    }
}

impl fmt::Display for SingleLayer {
    /// Formats the `SingleLayer` drain for display.
    ///
    /// This implementation provides a human-readable representation of the drain,
    /// including its domain patterns, similarity threshold, and a list of all
    /// contained log groups.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write into.
    ///
    /// # Returns
    ///
    /// A `fmt::Result` indicating success or failure of the formatting operation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let base = format!(
            "SimpleDrain\nDomain Patterns: {:?}\nSimilarity Threshold: {}\n",
            self.domain, self.threshold
        );
        let lg = "Log Groups:\n".to_string();
        let groups = self
            .iter_groups()
            .iter()
            .flatten()
            .map(std::string::ToString::to_string)
            .collect::<Vec<String>>();
        let group_str = groups.iter().join_with("\n");
        write!(f, "{}", [base, lg, group_str.to_string()].join_concat())
    }
}

#[cfg(test)]
mod should {
    use spectral::prelude::*;
    use tracing_test::traced_test;

    use crate::drains::{api::Drain, simple::SingleLayer}; // Removed <BucketBackend> for now, will add if compiler complains

    #[traced_test]
    #[test]
    fn test_new_drain() {
        let drain = SingleLayer::new(vec![]);
        assert_that(&drain).is_ok();
    }

    #[traced_test]
    #[test]
    fn test_set_threshold() {
        let mut drain = SingleLayer::new(vec![]).unwrap();
        let res = drain.set_threshold(100, 200);
        assert_that(&res).is_ok();
    }

    #[traced_test]
    #[test]
    fn test_single_process_line() {
        let mut drain = SingleLayer::new(vec![]).unwrap();
        let line_1 = "Message send failed to remote host: foo.bar.com".to_string();
        let res = drain.process_line(line_1);
        assert_that(&res).is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_multiple_process_line() {
        let mut drain = SingleLayer::new(vec![]).unwrap();
        let line_1 = "Message send failed to remote host: foo.bar.com".to_string();
        let line_2 = "Message send failed to remote host: bork.bork.com".to_string();
        let line_3 = "Unknown error received from peer".to_string();
        let res = drain.process_line(line_1);
        assert_that(&res).is_ok_containing(true);
        let res = drain.process_line(line_2);
        assert_that(&res).is_ok_containing(false);
        let res = drain.process_line(line_3);
        assert_that!(res).is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_iter_groups() {
        let line_1 = "This is a sequence".to_string();
        let line_2 = "Another different order of words".to_string();
        let line_3 = "Finally one last unique set of character runs".to_string();
        let mut drain = SingleLayer::new(vec![]).unwrap();
        Drain::process_line(&mut drain, line_1).unwrap();
        Drain::process_line(&mut drain, line_2).unwrap();
        Drain::process_line(&mut drain, line_3).unwrap();
        let groups = drain.collect_log_groups();
        assert_that(&groups).has_length(3);
    }
}
