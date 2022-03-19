pub mod log_group;
pub mod record;

use anyhow::{anyhow, Error};
use fraction::{BigInt, Ratio};
use log_group::LogGroup;
use record::Record;
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SimpleDrain<'a> {
    pub domain: Vec<Regex>,
    // NumTokens -> First Token -> List of Log groups
    base_layer: HashMap<usize, HashMap<String, Vec<LogGroup<'a>>>>,
    pub threshold: Ratio<BigInt>,
}

impl<'a> SimpleDrain<'a> {
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

    /// Accepts a line of input for processing against existing records
    ///
    /// Return
    /// Ok(true) when a new entry is added
    /// Ok(false) when the line matched an existing entry
    /// Err(e) for errors during processing
    pub fn process_line(&mut self, line: &'a str) -> Result<bool, Error> {
        let new_record = Record::new(line.clone());
        let length = new_record.len();
        let first = new_record
            .first()
            .expect("records have first tokens")
            .to_owned();
        // let log_groups = self.base_layer
        //     .entry(length)
        //     .or_insert_with( || {
        //         let mut map = HashMap::new();
        //         map.insert(
        //             new_record.first().expect("").to_owned(),
        //             vec![LogGroup::new(new_record.clone())],
        //         );
        //         map
        //     });
        if let Some(second_layer) = self.base_layer.get_mut(&length) {
            if let Some(log_groups) = second_layer.get_mut(&first as &str) {
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
        } else {
            self.base_layer.insert(length, HashMap::new());
            let second_layer = self
                .base_layer
                .get_mut(&length)
                .expect("We just inserted this map");
            second_layer.insert(first, vec![LogGroup::new(new_record)]);
            return Ok(true);
        }
        Err(anyhow!("Unspecified error occurred"))
    }
}

#[cfg(test)]
mod should {
    use crate::SimpleDrain;
    use spectral::prelude::*;

    #[test]
    fn test_new_drain() {
        let drain = SimpleDrain::new(vec![]);
        assert_that(&drain).is_ok();
    }

    #[test]
    fn test_single_process_line() {
        let mut drain = SimpleDrain::new(vec![]).unwrap();
        let line_1 = "Message send failed to remote host: foo.bar.com";
        let res = drain.process_line(line_1);
        assert_that(&res).is_ok_containing(true);
    }

    #[test]
    fn test_multiple_process_line() {
        let mut drain = SimpleDrain::new(vec![]).unwrap();
        let line_1 = "Message send failed to remote host: foo.bar.com";
        let line_2 = "Message send failed to remote host: bork.bork.com";
        let line_3 = "Unknown error received from peer";
        let res = drain.process_line(line_1);
        assert_that(&res).is_ok_containing(true);
        let res = drain.process_line(line_2);
        assert_that(&res).is_ok_containing(false);
        let res = drain.process_line(line_3);
        assert_that!(res).is_ok_containing(true);
    }
}
