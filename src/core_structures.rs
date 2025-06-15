//! # Core Data Structures for Log Processing
//!
//! This module defines the fundamental data structures used throughout the log processing pipeline.
//! These structures are utilized by the DRAIN parser for log clustering and template generation,
//! and by the differential dataflow runtime for analysis and aggregation of log data.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet; // Keep for now, not used by Vec<String>
use std::net::IpAddr;
use uuid::Uuid;
use std::hash::{Hash, Hasher};
use std::cmp::Ordering;

/// Wrapper for `f64` to implement `Eq`, `Ord`, and `Hash`.
/// This is necessary because standard `f64` does not provide a total order
/// (due to NaN values) and thus cannot directly derive `Eq` or `Ord`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, PartialOrd)]
pub struct FloatWrapper(
    /// The wrapped `f64` value.
    pub f64
);

impl Eq for FloatWrapper {}

impl Ord for FloatWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).unwrap_or_else(|| {
            if self.0.is_nan() && other.0.is_nan() {
                Ordering::Equal
            } else if self.0.is_nan() {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        })
    }
}

impl Hash for FloatWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        if self.0.is_nan() {
            0_u64.hash(state);
        } else {
            self.0.to_bits().hash(state);
        }
    }
}

/// Represents a raw log entry as it is ingested into the system.
/// This is the initial input format before any parsing or structuring.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RawLogEntry {
    /// Timestamp of when the log event occurred, in UTC.
    pub timestamp: DateTime<Utc>,
    /// The original, unstructured log message content.
    pub message: String,
    // Optional: a unique ID for the raw message if available from source
    // pub source_id: Option<String>,
}

/// Enum representing the various types of parameter values that can be extracted
/// from log lines during parsing. These values correspond to the variable parts
/// of a log template.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum ParameterValue {
    /// A string value.
    String(String),
    /// An integer value.
    Int(i64),
    /// A floating-point value, wrapped to provide total ordering and hashing.
    Float(FloatWrapper),
    /// An IP address (either IPv4 or IPv6).
    IpAddr(IpAddr),
    /// A timestamp value.
    Timestamp(DateTime<Utc>),
    /// A collection of related string tags or identifiers.
    /// Used for parameters that might represent a set of items like hostnames or database names.
    TagSet(Vec<String>),
    /// A boolean value.
    Boolean(bool),
    /// A fallback type for values that do not fit any of the other predefined types,
    /// or for parameters that have not yet been successfully typed.
    Other(String),
}

/// Represents a structured log entry after it has been processed by the DRAIN parser.
/// It links the log event to a specific template and contains the extracted parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ParsedLogEntry {
    /// Timestamp of the log event.
    pub timestamp: DateTime<Utc>,
    /// Identifier (UUID) of the `LogTemplate` that this log entry matches.
    pub template_id: Uuid,
    /// Vector of extracted parameter values, corresponding to the wildcards
    /// in the matched `LogTemplate`. The order of parameters is significant.
    pub parameters: Vec<ParameterValue>,
}

/// Represents a token within a log template.
/// A log template is composed of a sequence of these tokens.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TemplateToken {
    /// A literal string part of the template that is constant.
    Literal(String),
    /// A wildcard placeholder representing a variable part of the log message.
    Wildcard {
        /// A generated name for the wildcard (e.g., "param0", "ip_address_1").
        name: String,
        /// A hint about the expected data type of this wildcard (e.g., "String", "Base10Integer").
        type_hint: String
    },
}

/// Represents a unique log template identified by the DRAIN algorithm.
/// A template consists of a sequence of literal tokens and wildcards.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogTemplate {
    /// Unique identifier (UUID) for this log template.
    pub id: Uuid,
    /// The sequence of `TemplateToken`s (literals and wildcards) that form this template.
    pub tokens: Vec<TemplateToken>,
    // Optional: could store an example raw string that generated this template
    // pub example_raw_string: Option<String>,
    // Optional: could store the number of times this template has been seen
    // pub occurrence_count: u64,
}
