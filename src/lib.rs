pub mod log_group;
pub mod record;

use anyhow::Error;
use fraction::{BigInt, Ratio};
use log_group::LogGroup;
use record::Record;
use regex::Regex;
use std::collections::HashMap;

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

    pub fn process_line(mut self, line: &'a str) -> Result<bool, Error> {
        let mut found_new = false;
        let new_record = Record::new(line);
        if let Some(second_layer) = self.base_layer.get_mut(&new_record.len()) {
            if let Some(log_groups) =
                second_layer.get_mut(new_record.first().expect("records have first tokens"))
            {
                let (score, offset) = log_groups.into_iter().enumerate().fold(
                    (
                        0, // best score
                        0, // index of best score LogGroup
                    ),
                    |mut acc, elem| {
                        let score = new_record.clone().calc_sim_score(&elem.1.event());
                        if score > acc.0 {
                            acc = (score, elem.0); // overwrite state with new values
                        }
                        acc
                    },
                );
                let score_ratio =
                    Ratio::<BigInt>::new(BigInt::from(score), BigInt::from(new_record.len()));
                match score_ratio > self.threshold {
                    true => {
                        // add this record's uid to the list of examples for the log group
                        log_groups[offset].add_example(new_record);
                        // TODO: do token update pass
                    }
                    false => {
                        log_groups.push(LogGroup::new(new_record));
                        found_new = true;
                    }
                }
            }
        }
        Ok(found_new)
    }
}
