pub mod log_group;
pub mod record;

use std::collections::HashMap;
use std::fmt;

use anyhow::{anyhow, Error};
use fraction::{BigInt, FromPrimitive, Ratio};
use log_group::LogGroup;
use record::Record;
use regex::Regex;
use tracing::instrument;
#[derive(Debug, Clone)]
pub struct SimpleDrain {
    pub domain: Vec<Regex>,
    // NumTokens -> First Token -> List of Log groups
    base_layer: HashMap<usize, HashMap<String, Vec<LogGroup>>>,
    pub threshold: Ratio<BigInt>,
}

impl<'a> SimpleDrain {
    #[instrument]
    pub fn new(domain: Vec<String>) -> Result<Self, Error> {
        let patterns = domain
            .iter()
            .map(|s| Regex::new(&s))
            .collect::<Result<Vec<Regex>, regex::Error>>()?;
        Ok(Self {
            domain: patterns,
            base_layer: HashMap::new(),
            threshold: Ratio::from_float::<f32>(0.5).expect("0.5 converts into a ratio"),
        })
    }

    #[instrument]
    pub fn set_threshold(&mut self, numerator: u64, denominator: u64) -> Result<(), Error> {
        let numer = BigInt::from_u64(numerator)
            .ok_or(anyhow!("unable to make numerator from {}", numerator))?;
        let denom = BigInt::from_u64(denominator)
            .ok_or(anyhow!("unable to make denominator from {}", denominator))?;
        let new_ratio = Ratio::new(numer, denom);
        self.threshold = new_ratio;
        Ok(())
    }

    /// Accepts a line of input for processing against existing records
    ///
    /// Return
    /// Ok(true) when a new entry is added
    /// Ok(false) when the line matched an existing entry
    /// Err(e) for errors during processing
    #[instrument]
    pub fn process_line(&mut self, line: String) -> Result<bool, Error> {
        let new_record = Record::new(line);
        let length = new_record.len();
        let first = new_record
            .resolve(new_record.first())
            .expect("records have first tokens");
        match self.base_layer.get_mut(&length) {
            Some(second_layer) => {
                match second_layer.get_mut(&first as &str) {
                    Some(log_groups) => {
                        let (score, offset) = log_groups.into_iter().enumerate().fold(
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
                        let score_ratio = Ratio::<BigInt>::new(BigInt::from(score), BigInt::from(length));
                        match score_ratio > self.threshold {
                            true => {
                                // add this record's uid to the list of examples for the log group
                                log_groups[offset].add_example(new_record);
                                return Ok(false);
                            }
                            false => {
                                log_groups.push(LogGroup::new(new_record));
                                return Ok(true);
                            }
                        }
                    }
                    None => {
                        second_layer.insert(first, vec![LogGroup::new(new_record)]);
                        return Ok(true);
                    }
                }
            }
            _ => {
                    self.base_layer.insert(length, HashMap::new());
                    let second_layer = self
                        .base_layer
                        .get_mut(&length)
                        .expect("We just inserted this map");
                    second_layer.insert(first.to_string(), vec![LogGroup::new(new_record)]);
                    return Ok(true);
                }
        }
    }

    pub fn iter_groups(&self) -> Vec<Vec<&LogGroup>> {
        let mut results: Vec<Vec<&LogGroup>> = Vec::new();
        for length in self.base_layer.keys() {
            let mut groups = vec![];
            for (_, grp) in self.base_layer.get(length).unwrap().iter() {
                for g in grp {
                    groups.push(g)
                }
            }
            results.push(groups);
        }
        results
    }
}

impl fmt::Display for SimpleDrain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SimpleDrain\nDomain Patterns: {:?}\nSimilarity Threshold: {}\n",
            self.domain, self.threshold
        )
    }
}

#[cfg(test)]
mod should {
    use crate::SimpleDrain;
    use spectral::prelude::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_new_drain() {
        let drain = SimpleDrain::new(vec![]);
        assert_that(&drain).is_ok();
    }

    #[traced_test]
    #[test]
    fn test_set_threshold() {
        let mut drain = SimpleDrain::new(vec![]).unwrap();
        let res = drain.set_threshold(100, 200);
        assert_that(&res).is_ok();
    }

    #[traced_test]
    #[test]
    fn test_single_process_line() {
        let mut drain = SimpleDrain::new(vec![]).unwrap();
        let line_1 = "Message send failed to remote host: foo.bar.com".to_string();
        let res = drain.process_line(line_1);
        assert_that(&res).is_ok_containing(true);
    }

    #[traced_test]
    #[test]
    fn test_multiple_process_line() {
        let mut drain = SimpleDrain::new(vec![]).unwrap();
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
        let mut drain = SimpleDrain::new(vec![]).unwrap();
        drain.process_line(line_1).unwrap();
        drain.process_line(line_2).unwrap();
        drain.process_line(line_3).unwrap();
        let groups = drain.iter_groups();
        assert_that(&groups).has_length(3);
    }
}
