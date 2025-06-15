use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::IpAddr;
use uuid::Uuid;

// Raw log entry fed into the system
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RawLogEntry {
    pub timestamp: DateTime<Utc>,
    pub message: String,
    // Optional: a unique ID for the raw message if available from source
    // pub source_id: Option<String>,
}

// Enum for typed parameters extracted from log lines
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum ParameterValue {
    String(String),
    Int(i64),
    Float(f64),
    IpAddr(IpAddr),
    Timestamp(DateTime<Utc>),
    TagSet(HashSet<String>), // For sets of related string values like hostnames, db names
    Boolean(bool),
    // Fallback for values that don't fit predefined types or for initial untyped extraction
    Other(String),
}

// Structured log entry after DRAIN parsing
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ParsedLogEntry {
    pub timestamp: DateTime<Utc>, // Timestamp of the log event
    pub template_id: Uuid,        // Identifier for the log template
    // Extracted parameters, order corresponds to wildcards in the template
    pub parameters: Vec<ParameterValue>,
}

// Represents a token in a log template
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TemplateToken {
    Literal(String), // A fixed string part of the template
    // A wildcard placeholder. 'name' could be a generated name like "var1", "ip_address_1"
    // 'parameter_type_hint' could be an enum similar to ParameterValue variants but representing the type class
    // For now, using a String to represent the type for simplicity, can be refined later.
    Wildcard { name: String, type_hint: String },
}

// Represents a unique log template
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LogTemplate {
    pub id: Uuid, // Unique identifier for this template
    pub tokens: Vec<TemplateToken>, // The sequence of tokens forming the template
                  // Optional: could store an example raw string that generated this template
                  // pub example_raw_string: Option<String>,
                  // Optional: could store the number of times this template has been seen
                  // pub occurrence_count: u64,
}
