#![allow(non_local_definitions)]

use abomonation_derive::Abomonation;
use serde::{Deserialize, Serialize};

#[derive(Abomonation, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RawLog {
    pub id: usize,
    pub content: String,
}

#[derive(Abomonation, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TokenizedLog {
    pub original_id: usize,
    pub tokens: Vec<String>,
    pub token_count: usize,
}

#[derive(Abomonation, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LogTemplate {
    pub template_id: usize,
    // A mix of concrete tokens and a special wildcard token
    pub token_spurs: Vec<String>,
    pub token_count: usize,
}

#[derive(Abomonation, Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LogCluster {
    pub template_id: usize,
    pub token_spurs: Vec<String>,
    pub event_count: u64,
    // Include a few sample raw log lines for context
    pub samples: Vec<String>,
}
