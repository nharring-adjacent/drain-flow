// src/drains/differential_drain.rs

use serde::{Serialize, Deserialize};
use uuid::Uuid;
// Potentially need to add `use string_interner::DefaultSymbol;` if we use it for tokens directly in ProcessedLogMessage
// For now, let's assume tokens are Strings or a similar type that doesn't require DefaultSymbol directly in struct defs yet.

/// Represents a raw log entry.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogMessage {
    pub timestamp: u64, // Or chrono::DateTime<chrono::Utc> if more precision/timezone handling is needed
    pub content: String,
    // Potentially an ID if logs come with a unique identifier from the source
    // pub source_id: Option<String>,
}

/// Represents a log message after preprocessing and tokenization.
/// Tokens are expected to be interned strings, but stored as actual strings or symbols.
#[derive(Clone, Debug)]
pub struct ProcessedLogMessage {
    pub original_message_id: Uuid, // Link back to an original LogMessage or a unique ID generated for it
    pub tokens: Vec<String>, // Or Vec<DefaultSymbol> if using string_interner directly here
    // pub length: usize, // Can be derived from tokens.len()
}

/// Enum representing either a specific token (interned string) or a wildcard.
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Clone, Debug)]
pub enum TokenOrWildcard {
    Token(String), // Or DefaultSymbol
    Wildcard,
    // Potentially more specific wildcards, e.g., WildcardNumeric, WildcardAlphanum
}

/// Represents a DRAIN log cluster.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogCluster {
    pub cluster_id: Uuid,
    pub log_template: Vec<TokenOrWildcard>,
    // Store a few representative ProcessedLogMessage (or their IDs/content)
    // For simplicity, let's store the full ProcessedLogMessage for now.
    // In a high-volume system, storing only IDs or a compressed representation might be better.
    pub samples: Vec<ProcessedLogMessage>, // Could also be Vec<Uuid> referring to ProcessedLogMessage IDs
    pub count: u64,
    // Potentially add:
    // pub first_seen: u64, // Timestamp of the first message in this cluster
    // pub last_seen: u64, // Timestamp of the most recent message
}

impl LogCluster {
    pub fn new(initial_message: ProcessedLogMessage, template: Vec<TokenOrWildcard>) -> Self {
        LogCluster {
            cluster_id: Uuid::new_v4(),
            log_template: template,
            samples: vec![initial_message],
            count: 1,
        }
    }
}

// TODO: Add to src/drains/mod.rs: `pub mod differential_drain;`
// TODO: Add dependencies to Cargo.toml if not already present:
// uuid = { version = "1.0", features = ["serde", "v4"] }
// serde = { version = "1.0", features = ["derive"] }
// string_interner = "..." (if using DefaultSymbol directly in structs)

