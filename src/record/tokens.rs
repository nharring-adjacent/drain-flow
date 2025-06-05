use lazy_static::lazy_static;
use regex::Regex;
// Ensure INTERNER is accessible for TokenStream::from_unicode_line - NO LONGER NEEDED HERE
// Assuming simple::INTERNER is the correct path based on src/record/mod.rs
// use crate::drains::simple::INTERNER; // Removed unused import
use crate::drains::differential_drain::TokenOrWildcard;


#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenType {
    IpAddress,
    Hostname,
    Integer,
    Float,
    Date,
    Time,
    FilePath,
    AlphanumericId,
    BooleanStatus,
    UrlPath,
    Word,
    Wildcard, // Represents <*>
    Other,
}

// Added definitions for Offset and TokenStream to resolve compilation errors
pub type Offset = usize;

#[derive(Clone, Debug)]
pub struct TokenStream {
    pub inner: Vec<(Offset, TokenOrWildcard)>,
}

impl TokenStream {
    pub fn from_unicode_line(line: &str) -> Self {
        // Removed unused interner variable
        let tokens = line
            .split_whitespace()
            .enumerate() // Using enumerate for a simple offset
            .map(|(offset, s)| {
                (
                    offset,
                    // TokenOrWildcard::Token expects a String. 's' is &str from split_whitespace.
                    TokenOrWildcard::Token(s.to_string()),
                )
            })
            .collect();
        Self { inner: tokens }
    }

    pub fn first(&self) -> Option<&TokenOrWildcard> {
        self.inner.first().map(|(_, token)| token)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn get_token_at_index(&self, index: usize) -> Option<&TokenOrWildcard> {
        self.inner.get(index).map(|(_, token)| token)
    }
}

impl std::fmt::Display for TokenStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut first = true;
        for (_offset, token) in &self.inner {
            if !first {
                write!(f, " ")?;
            }
            write!(f, "{}", token)?; // Relies on TokenOrWildcard having a Display impl
            first = false;
        }
        Ok(())
    }
}
// End of added definitions for Offset and TokenStream

lazy_static! {
    // Order matters: more specific regexes should come before general ones.
    static ref IP_ADDRESS_REGEX: Regex = Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$").unwrap();
    static ref HOSTNAME_REGEX: Regex = Regex::new(r"^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$").unwrap();
    static ref INTEGER_REGEX: Regex = Regex::new(r"^[+-]?\d+$").unwrap();
    static ref FLOAT_REGEX: Regex = Regex::new(r"^[+-]?\d+\.\d+$").unwrap();
    static ref DATE_REGEX: Regex = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap(); // YYYY-MM-DD
    static ref TIME_REGEX: Regex = Regex::new(r"^\d{2}:\d{2}:\d{2}$").unwrap(); // HH:MM:SS
    static ref FILE_PATH_REGEX: Regex = Regex::new(r"^(/[a-zA-Z0-9_.-]+)+/?$").unwrap();
    static ref ALPHANUMERIC_ID_REGEX: Regex = Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$|^(0x[0-9a-fA-F]+|[a-zA-Z0-9_-]*[a-zA-Z][a-zA-Z0-9_-]*\d+[a-zA-Z0-9_-]*|[a-zA-Z0-9_-]*\d+[a-zA-Z0-9_-]*[a-zA-Z][a-zA-Z0-9_-]*)$").unwrap();
    static ref BOOLEAN_STATUS_REGEX: Regex = Regex::new(r"(?i)^(true|false|success|failure|error|warn|info|debug|ok|failed|yes|no|enabled|disabled)$").unwrap();
    static ref URL_PATH_REGEX: Regex = Regex::new(r"^(/[a-zA-Z0-9_.-]+)+/?(\?[a-zA-Z0-9_=&%-]*)?$").unwrap();
    static ref WORD_REGEX: Regex = Regex::new(r"^[a-zA-Z]+$").unwrap();
}

pub fn get_token_type(token_str: &str) -> TokenType {
    if token_str == "<*>" {
        return TokenType::Wildcard;
    }
    if IP_ADDRESS_REGEX.is_match(token_str) {
        return TokenType::IpAddress;
    }
    if HOSTNAME_REGEX.is_match(token_str) {
        if token_str.contains('.') || token_str.contains('-') || (!INTEGER_REGEX.is_match(token_str) && !WORD_REGEX.is_match(token_str)) {
            return TokenType::Hostname;
        }
    }
    if INTEGER_REGEX.is_match(token_str) {
        return TokenType::Integer;
    }
    if FLOAT_REGEX.is_match(token_str) {
        return TokenType::Float;
    }
    if DATE_REGEX.is_match(token_str) {
        return TokenType::Date;
    }
    if TIME_REGEX.is_match(token_str) {
        return TokenType::Time;
    }
    if FILE_PATH_REGEX.is_match(token_str) {
        return TokenType::FilePath;
    }
    if URL_PATH_REGEX.is_match(token_str) {
        return TokenType::UrlPath;
    }
    if ALPHANUMERIC_ID_REGEX.is_match(token_str) {
        if !WORD_REGEX.is_match(token_str) && !INTEGER_REGEX.is_match(token_str) && (token_str.contains('-') || token_str.contains('_') || token_str.chars().any(char::is_numeric) && token_str.chars().any(char::is_alphabetic)) {
             return TokenType::AlphanumericId;
        }
    }
    if BOOLEAN_STATUS_REGEX.is_match(token_str) {
        return TokenType::BooleanStatus;
    }
    if WORD_REGEX.is_match(token_str) {
        return TokenType::Word;
    }
    TokenType::Other
}
