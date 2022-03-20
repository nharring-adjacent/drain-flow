use tracing::info;

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
    pub fn new(event: Record) -> Self {
        info!("new record");
        Self {
            event: event,
            examples: vec![],
            wildcards: vec![],
        }
    }

    pub fn add_example(&mut self, rec: Record) {
        info!(?rec, "add example");
        self.examples.push(rec);
    }

    pub fn event(&self) -> &Record {
        &self.event
    }
}
