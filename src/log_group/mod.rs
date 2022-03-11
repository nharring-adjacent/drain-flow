use crate::record::Record;

#[derive(Clone, Debug)]
pub struct LogGroup<'a> {
    event: Record<'a>,
    examples: Vec<Record<'a>>,
    wildcards: Vec<usize>,
}

impl<'a> LogGroup<'a> {
    pub fn new(event: Record<'a>) -> Self {
        Self {
            event: event,
            examples: vec![],
            wildcards: vec![],
        }
    }

    pub fn add_example(&mut self, rec: Record<'a>) {
        self.examples.push(rec);
    }
    
    pub fn event(&self) -> &Record<'a> {
        &self.event
    }
}