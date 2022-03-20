use crate::record::{tokens::Token, Record};

#[derive(Clone, Debug)]
pub struct LogGroup {
    event: Record,
    examples: Vec<Record>,
    wildcards: Vec<Wildcard>,
}

type Wildcard = (usize, Token);

impl LogGroup {
    pub fn new(event: Record) -> Self {
        Self {
            event: event,
            examples: vec![],
            wildcards: vec![],
        }
    }

    pub fn add_example(&mut self, rec: Record) {
        self.examples.push(rec);
    }

    pub fn event(&self) -> &Record {
        &self.event
    }
}

impl Iterator for LogGroup {
    type Item = Wildcard;
    fn next(&mut self) -> Option<Wildcard> {
        None
    }
}
