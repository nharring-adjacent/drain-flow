use chrono::{DateTime, Utc};
use lasso::Spur;
use serde::{Deserialize, Serialize};
use timely::order::Product;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RawLog {
    pub content: String,
    // Original timestamp can be part of the dataflow timestamp
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct TokenizedLog {
    pub original_id: Uuid, // A unique ID for traceability
    pub tokens: Vec<Spur>,
    pub token_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LogTemplate {
    pub template_id: Uuid,
    // A mix of concrete tokens and a special wildcard token
    pub template_tokens: Vec<Spur>,
    pub token_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Hash)]
pub struct LogCluster {
    pub template_id: Uuid,
    pub template_tokens: Vec<Spur>,
    pub event_count: u64,
    // Include a few sample raw log lines for context
    pub samples: Vec<String>,
}
