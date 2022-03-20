use std::fmt;
use tracing::instrument;

use crate::record::{tokens::Token, Record};

#[derive(Clone, Debug)]
pub struct LogGroup {
    event: Record,
    examples: Vec<Record>,
    pub wildcards: Vec<Wildcard>,
}

/// A wildcard is a token position and token type
type Wildcard = (usize, Token);

impl LogGroup {
    #[instrument]
    pub fn new(event: Record) -> Self {
        Self {
            event: event,
            examples: vec![],
            wildcards: vec![],
        }
    }

    #[instrument]
    pub fn add_example(&mut self, rec: Record) {
        self.examples.push(rec);
    }

    #[instrument]
    pub fn event(&self) -> &Record {
        &self.event
    }
}

impl fmt::Display for LogGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LogGroup {:?}\nEvent: {}\n{} examples and {} wildcards\n",
            self.event.uid,
            self.event.to_string(),
            self.examples.len(),
            self.wildcards.len()
        )
    }
}
