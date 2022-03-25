use std::{borrow::Borrow, collections::HashMap, fmt};

use anyhow::Error;
use tracing::{info, instrument};

use crate::record::{tokens::Token, Record};

#[derive(Clone, Debug)]
pub struct LogGroup {
    event: Record,
    examples: Vec<Record>,
    pub variables: HashMap<usize, Token>,
}

/// A wildcard is an offset and a token
pub type Wildcard = (usize, Token);

impl LogGroup {
    #[instrument]
    pub fn new(event: Record) -> Self {
        Self {
            event,
            examples: vec![],
            variables: HashMap::new(),
        }
    }

    #[instrument]
    pub fn add_example(&mut self, rec: Record) {
        let vars = self.discover_variables(&rec).unwrap();
        self.examples.push(rec);
        if !vars.is_empty() {
            self.updaate_variables(vars);
        }
    }

    #[instrument]
    pub fn event(&self) -> &Record {
        &self.event
    }

    pub fn discover_variables(&self, rec: &Record) -> Result<Vec<Wildcard>, Error> {
        let f = self
            .event
            .borrow()
            .into_iter()
            .enumerate()
            .zip(rec.into_iter())
            .filter(|((idx, event), candidate)| {
                if let Some(_) = self.variables.get(idx) {
                    // This token has already been identified as a variable
                    false
                } else if event != candidate {
                    info!(%idx, ?event, ?candidate, "found candidate");
                    true
                } else {
                    false
                }
            })
            .filter_map(|((idx, _event), _candidate)| Some((idx, Token::Wildcard)))
            .collect::<Vec<_>>();
        Ok(f)
    }

    fn updaate_variables(&mut self, vars: Vec<(usize, Token)>) {
        for var in vars {
            // Assume we got vars from discover_variab les so it has already checked against this map
            self.variables.insert(var.0, var.1);
        }
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
    use crate::{
        log_group::LogGroup,
        record::{tokens::Token, Record},
    };

    use spectral::prelude::*;
    use std::collections::HashMap;

    #[test]
    fn test_discover_variables() {
        let rec1 = Record::new("Common prefix Common prefix Common prefix 1234".to_string());
        let lg = LogGroup {
            event: rec1.clone(),
            examples: vec![rec1],
            variables: HashMap::new(),
        };
        let rec2 = Record::new("Common prefix Common prefix Common prefix 3456".to_string());
        let vars = lg.discover_variables(&rec2);
        assert_that(&vars).is_ok_containing(vec![(6, Token::Wildcard)]);
    }

    #[test]
    fn test_update_variables() {
        let rec1 = Record::new("Common prefix Common prefix Common prefix 1234".to_string());
        let mut lg = LogGroup {
            event: rec1.clone(),
            examples: vec![rec1],
            variables: HashMap::new(),
        };
        let rec2 = Record::new("Common prefix Common prefix Common prefix 3456".to_string());
        let vars = lg.discover_variables(&rec2).unwrap();
        lg.updaate_variables(vars);
        assert_that(&lg.variables).contains_key(6);
    }

    // prop_compose! {
    //     fn generate_line_pair(words: usize, variables: usize) -> (String, String) {

    //     }
    // }
}
