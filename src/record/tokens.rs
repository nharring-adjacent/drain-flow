// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::{
    collections::HashMap,
    fmt::{self, Display},
};

use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_static::lazy_static;
use regex::RegexSet;
use string_interner::DefaultSymbol;
use tracing::{debug, instrument};

pub use super::ASTERISK; // Made ASTERISK re-export public
use crate::drains::simple::INTERNER;

lazy_static! {
    static ref MATCHERS: RegexSet = Grokker::build_pattern_set();
    static ref GROKKER_COUNT: usize = Grokker::iter_variants().count() - 1;
    static ref GROKKER_SYMS: HashMap<Grokker, DefaultSymbol> = symbolize_grokker();
    static ref GROKKER_VARIANTS: HashMap<usize, Grokker> = Grokker::iter_variants()
        .enumerate()
        .collect::<HashMap<usize, Grokker>>();
}

fn symbolize_grokker() -> HashMap<Grokker, DefaultSymbol> {
    Grokker::iter_variants()
        .map(|v| (v, INTERNER.write().get_or_intern(v.to_string())))
        .collect::<HashMap<Grokker, DefaultSymbol>>()
}

/// An enumeration of various data patterns (groks) that can be identified in log tokens.
///
/// Each variant represents a specific type of data, such as integers, floats, UUIDs,
/// MAC addresses, IP addresses, hostnames, months, and days.
custom_derive! {
    #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, IterVariants(GrokkerVariants), EnumDisplay)]
    pub enum Grokker {
        /// Matches base-10 integer numbers.
        Base10Integer,
        /// Matches base-10 floating-point numbers.
        Base10Float,
        /// Matches base-16 (hexadecimal) integer numbers.
        Base16Integer,
        /// Matches base-16 (hexadecimal) floating-point numbers.
        Base16Float,
        /// Matches UUIDs (Universally Unique Identifiers).
        UUID,
        /// Matches MAC addresses.
        MAC,
        /// Matches IPv6 addresses.
        IPv6,
        /// Matches IPv4 addresses.
        IPv4,
        /// Matches hostnames.
        Hostname,
        /// Matches month names (e.g., Jan, January).
        Month,
        /// Matches day names (e.g., Mon, Monday).
        Day,
    }
}

impl Grokker {
    /// Returns the regular expression pattern string for the given `Grokker` variant.
    ///
    /// # Returns
    ///
    /// A `String` containing the regular expression pattern.
    #[must_use]
    pub fn to_pattern(self) -> String {
        match self {
            Grokker::Base10Integer => r"^(?:[+-]?(?:[0-9]+))$".to_string(),
            Grokker::Base10Float => {
                r"^(?:[+-]?(?:(?:[0-9]+(?:\.[0-9]+))|(?:\.[0-9]+)))$".to_string()
            }
            Grokker::Base16Integer => r"^(?:[+-]?(?:0x)?(?:[0-9A-Fa-f]+))$".to_string(),
            Grokker::Base16Float => {
                r"^(?:[+-]?(?:0x)?(?:[0-9A-Fa-f]+)(?:\.[0-9A-Fa-f]+))$".to_string()
            }
            Grokker::UUID => r"^[A-Fa-f0-9]{8}-(?:[A-Fa-f0-9]{4}-){3}[A-Fa-f0-9]{12}$".to_string(),
            Grokker::MAC => r"^(?:(?:[A-Fa-f0-9]{2}:){5}[A-Fa-f0-9]{2})$".to_string(),
            Grokker::IPv6 => {
                r"^((([0-9A-Fa-f]{1,4}:){7}([0-9A-Fa-f]{1,4}|:))|(([0-9A-Fa-f]{1,4}:){6}(:[0-9A-Fa-f]{1,4}|((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){5}(((:[0-9A-Fa-f]{1,4}){1,2})|:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){4}(((:[0-9A-Fa-f]{1,4}){1,3})|((:[0-9A-Fa-f]{1,4})?:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){3}(((:[0-9A-Fa-f]{1,4}){1,4})|((:[0-9A-Fa-f]{1,4}){0,2}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){2}(((:[0-9A-Fa-f]{1,4}){1,5})|((:[0-9A-Fa-f]{1,4}){0,3}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){1}(((:[0-9A-Fa-f]{1,4}){1,6})|((:[0-9A-Fa-f]{1,4}){0,4}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(:(((:[0-9A-Fa-f]{1,4}){1,7})|((:[0-9A-Fa-f]{1,4}){0,5}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:)))(%.+)?$".to_string()
            }
            Grokker::IPv4 => {
                r"^(?:(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])[.](?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])[.](?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])[.](?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5]))$".to_string()
            }
            Grokker::Hostname => {
                r"^(?:[0-9A-Za-z][0-9A-Za-z-]{0,62})(?:\.(?:[0-9A-Za-z][0-9A-Za-z-]{0,62}))*(\.?|\b)$".to_string()
            }
            Grokker::Month => {
                r"^(?:[Jj]an(?:uary|uar)?|[Ff]eb(?:ruary|ruar)?|[Mm](?:a|ä)?r(?:ch|z)?|[Aa]pr(?:il)?|[Mm]a(?:y|i)?|[Jj]un(?:e|i)?|[Jj]ul(?:y)?|[Aa]ug(?:ust)?|[Ss]ep(?:tember)?|[Oo](?:c|k)?t(?:ober)?|[Nn]ov(?:ember)?|[Dd]e(?:c|z)(?:ember)?)$".to_string()
            }
            Grokker::Day => {
                r"^(?:Mon(?:day)?|Tue(?:sday)?|Wed(?:nesday)?|Thu(?:rsday)?|Fri(?:day)?|Sat(?:urday)?|Sun(?:day)?)$".to_string()
            }
        }
    }

    /// Builds a `RegexSet` containing all patterns for `Grokker` variants.
    ///
    /// This set is used for efficient matching of a string against multiple patterns.
    ///
    /// # Returns
    ///
    /// A `RegexSet` containing all `Grokker` patterns.
    ///
    /// # Panics
    ///
    /// Panics if any of the `Grokker` patterns are invalid regular expressions.
    fn build_pattern_set() -> RegexSet {
        let variants = Grokker::iter_variants()
            .map(Grokker::to_pattern)
            .collect::<Vec<String>>();
        RegexSet::new(variants).expect("valid regular expressions compile")
    }

    /// Converts a match index from a `RegexSet` into a corresponding `Grokker` variant.
    ///
    /// # Arguments
    ///
    /// * `idx` - The index of the matched pattern in the `RegexSet`.
    ///
    /// # Returns
    ///
    /// An `Option<Grokker>` containing the `Grokker` variant if the index is valid,
    /// otherwise `None`.
    #[instrument(level = "trace")]
    pub fn from_match_index(idx: usize) -> Option<Grokker> {
        if idx > *GROKKER_COUNT {
            return None;
        }
        Some(GROKKER_VARIANTS[&idx])
    }
}

/// A convenience wrapper over `regex::RegexSet::matches` and `Grokker` variants.
///
/// `GrokSet` provides methods to check if a string matches certain predefined
/// data patterns (groks).
#[derive(Debug, Clone)]
pub struct GrokSet {
    match_types: Vec<Grokker>,
}

impl GrokSet {
    /// Creates a new `GrokSet` by analyzing the provided string `value`.
    ///
    /// It uses a pre-compiled `RegexSet` to determine which `Grokker` patterns
    /// the `value` matches.
    ///
    /// # Arguments
    ///
    /// * `value` - The string to analyze.
    ///
    /// # Returns
    ///
    /// A new `GrokSet` instance containing the types of `Grokker` patterns matched.
    #[must_use]
    pub fn new(value: &str) -> Self {
        let matches = MATCHERS.matches(value);
        let match_types: Vec<_> = matches
            .iter()
            .filter_map(Grokker::from_match_index)
            .collect();
        Self { match_types }
    }

    /// Checks if any of the matched `Grokker` types are numeric (integers or floats).
    ///
    /// # Returns
    ///
    /// `true` if the `GrokSet` contains any numeric `Grokker` type, `false` otherwise.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        self.match_types.iter().any(|i| {
            matches!(
                i,
                Grokker::Base10Integer
                    | Grokker::Base16Integer
                    | Grokker::Base16Float
                    | Grokker::Base10Float
            )
        })
    }

    /// Checks if any of the matched `Grokker` types are integers (base-10 or base-16).
    ///
    /// # Returns
    ///
    /// `true` if the `GrokSet` contains any integer `Grokker` type, `false` otherwise.
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.match_types
            .iter()
            .any(|i| matches!(i, Grokker::Base10Integer | Grokker::Base16Integer))
    }
}

/// Represents a token within a log line, which can be a wildcard, a typed match, or a specific value.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// A wildcard token, matching any other token. Represented as "<*>" in display.
    Wildcard,
    /// A token that matches any value of a specific predefined type (e.g., UUID, IPv4).
    TypedMatch(Grokker),
    /// A token containing a specific, non-wildcard value.
    Value(TypedToken),
}

impl Token {
    /// Parses an input string and attempts to classify it into a `Token` type.
    ///
    /// This function uses a set of regular expressions (`MATCHERS`) to determine
    /// if the input string matches any known `Grokker` patterns. It prioritizes
    /// more specific matches and handles ambiguities (e.g., a UUID also matching
    /// a hostname pattern).
    ///
    /// # Arguments
    ///
    /// * `input` - The string slice to parse.
    ///
    /// # Returns
    ///
    /// A `Token` representing the classification of the input string.
    #[instrument(level = "trace")]
    pub fn from_parse(input: &str) -> Token {
        let matches = MATCHERS.matches(input);
        let match_types: Vec<_> = matches
            .iter()
            .filter_map(Grokker::from_match_index)
            .collect();

        debug!("comparing {} tokens", match_types.len());

        let tok = match match_types.len() {
            0 => Token::Value(TypedToken::from_parse(input)),
            1 => {
                let idx = matches.iter().collect::<Vec<usize>>()[0];
                let grokker = Grokker::from_match_index(idx).unwrap();
                debug!(%grokker, "single match");
                Token::TypedMatch(grokker)
            }
            2 => {
                debug!(?match_types, "2 match arm");
                // UUID and hostname can overlap, if they do its 99.999% a UUID
                if match_types.contains(&Grokker::UUID) && match_types.contains(&Grokker::Hostname)
                {
                    debug!("uuid & hostname");
                    return Token::TypedMatch(Grokker::UUID);
                }
                // All base10 ints match base16 ints
                if match_types.contains(&Grokker::Base10Integer)
                    && match_types.contains(&Grokker::Base16Integer)
                {
                    return Token::TypedMatch(Grokker::Base10Integer);
                }
                // All base10 floats match base16 floats
                if match_types.contains(&Grokker::Base10Float)
                    && match_types.contains(&Grokker::Base16Float)
                {
                    debug!("base10 & base16 float");
                    return Token::TypedMatch(Grokker::Base10Float);
                }
                // base16 numbers and hostname can overlap, if they do its 99.999% a number
                if match_types.contains(&Grokker::Base16Integer)
                    && match_types.contains(&Grokker::Hostname)
                {
                    debug!("base16 int & hostname");
                    return Token::TypedMatch(Grokker::Base16Integer);
                }
                if match_types.contains(&Grokker::Base16Float)
                    && match_types.contains(&Grokker::Hostname)
                {
                    debug!("base16 float & hostname");
                    return Token::TypedMatch(Grokker::Base16Float);
                }
                debug!("fallback to wildcard");
                Token::Wildcard
            }
            3 => {
                debug!(?match_types, "3 match arm");
                // All base10 integers also match as base16 and weirdly as hostnames
                if match_types.contains(&Grokker::Base10Integer)
                    && match_types.contains(&Grokker::Base16Integer)
                    && match_types.contains(&Grokker::Hostname)
                {
                    debug!("base10 int mistaken for hostname");
                    return Token::TypedMatch(Grokker::Base10Integer);
                }

                if match_types.contains(&Grokker::Base10Float)
                    && match_types.contains(&Grokker::Base16Float)
                    && match_types.contains(&Grokker::Hostname)
                {
                    debug!("base 10 float mistaken for hostname");
                    return Token::TypedMatch(Grokker::Base10Float);
                }
                debug!("fallback to wildcard");
                Token::Wildcard
            }
            // Todo: Explore if there is a way to figure out a "best match"
            _ => Token::Wildcard,
        };
        tok
    }
}

impl fmt::Display for Token {
    /// Formats the `Token` for display.
    ///
    /// This implementation provides a string representation of the token,
    /// using "<*>" for `Wildcard` tokens, the `Grokker`'s display for `TypedMatch`,
    /// and the underlying value's string representation for `Value` tokens.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write into.
    ///
    /// # Returns
    ///
    /// A `fmt::Result` indicating success or failure of the formatting operation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out: String = match self {
            Token::Wildcard => "<*>".to_string(),
            Token::TypedMatch(t) => t.to_string(),
            Token::Value(v) => match v {
                TypedToken::String(sym) => INTERNER
                    .read()
                    .resolve(*sym)
                    .expect("symbols must resolve")
                    .to_string(),
                TypedToken::Int(i) => format!("{}", i),
                TypedToken::Float(f) => f.to_string(),
            },
        };
        write!(f, "{}", out)
    }
}

impl From<Token> for DefaultSymbol {
    /// Converts a `Token` into its `DefaultSymbol` representation.
    ///
    /// This conversion uses the global string interner to obtain a symbol
    /// for the token's string value or a predefined symbol for `Wildcard`
    /// and `TypedMatch` tokens.
    ///
    /// # Arguments
    ///
    /// * `tok` - The `Token` to convert.
    ///
    /// # Returns
    ///
    /// The `DefaultSymbol` corresponding to the `Token`.
    fn from(tok: Token) -> DefaultSymbol {
        match tok {
            Token::Wildcard => *ASTERISK,
            Token::TypedMatch(t) => *GROKKER_SYMS
                .get(&t)
                .expect("every grokker must have a symbol"),
            Token::Value(v) => match v {
                TypedToken::String(s) => s,
                TypedToken::Int(i) => INTERNER.write().get_or_intern(i.to_string()),
                TypedToken::Float(f) => INTERNER.write().get_or_intern(f.to_string()),
            },
        }
    }
}

/// Represents a typed token value.
///
/// This enum distinguishes between string, integer, and floating-point token values.
#[derive(PartialEq, Debug, Clone)]
pub enum TypedToken {
    /// A token containing a string value. This is typically used for words or phrases
    /// that do not match any specific numeric or other structured patterns.
    String(DefaultSymbol),
    /// A token containing a whole number.
    Int(i64),
    /// A token containing a floating-point number.
    Float(f64),
}

impl TypedToken {
    /// Parses a supplied string and returns a `TypedToken::String`.
    ///
    /// This function currently only interns the input string as a `DefaultSymbol`
    /// and wraps it in a `TypedToken::String`.
    ///
    /// # Arguments
    ///
    /// * `input` - The string slice to parse.
    ///
    /// # Returns
    ///
    /// A `TypedToken::String` containing the interned representation of the input.
    #[must_use]
    pub fn from_parse(input: &str) -> TypedToken {
        TypedToken::String(INTERNER.write().get_or_intern(input))
    }
}

/// Represents an offset within a string, indicating the start and end byte positions.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Offset {
    /// The starting byte position of the token.
    start: usize,
    /// The ending byte position of the token.
    end: usize,
}

impl Display for Offset {
    /// Formats the `Offset` for display.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write into.
    ///
    /// # Returns
    ///
    /// A `fmt::Result` indicating success or failure of the formatting operation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Offset(start: {}, end: {})", self.start, self.end)
    }
}

/// Represents a stream of tokens, typically derived from a log line.
///
/// A `TokenStream` stores a sequence of `(Offset, Token)` pairs, preserving
/// the original position and type of each token within the source string.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenStream {
    pub(crate) inner: Vec<(Offset, Token)>,
}

impl TokenStream {
    /// Creates a `TokenStream` from a Unicode log line.
    ///
    /// This function splits the input line by ASCII whitespace and attempts to
    /// identify and intern each word as a `Token::Value(TypedToken::String)`. It also
    /// calculates and stores the `Offset` for each token.
    ///
    /// # Arguments
    ///
    /// * `line` - The input log line as a string slice.
    ///
    /// # Returns
    ///
    /// A new `TokenStream` instance.
    #[instrument(skip(line))]
    pub fn from_unicode_line(line: &str) -> Self {
        let mut interner = INTERNER.write();
        let mut progress = 0usize;
        let words = line
            .split_ascii_whitespace()
            .filter_map(|w| {
                debug!(%w, %progress, "got");
                let start = line.match_indices(w).find(|(i, _w)| {
                    debug!(%progress, %i, "found");
                    i >= &progress
                })?;
                let end = start.0 + start.1.len();
                progress = end;
                let token = (
                    Offset {
                        start: start.0,
                        end,
                    },
                    Token::Value(TypedToken::String(interner.get_or_intern(w))),
                );
                debug!(?token, %w, ?start, "built");
                Some(token)
            })
            .collect::<Vec<(Offset, Token)>>();
        Self { inner: words }
    }

    /// Returns the first `Token` in the stream.
    ///
    /// # Returns
    ///
    /// An `Option<Token>` containing a clone of the first token if the stream is not empty,
    /// otherwise `None`.
    #[instrument(level = "trace", skip(self))]
    pub fn first(&self) -> Option<Token> {
        match self.inner.len() {
            0 => None,
            _ => Some(self.inner[0].1.clone()),
        }
    }

    /// Returns the number of tokens in the stream.
    ///
    /// # Returns
    ///
    /// The length of the token stream as a `usize`.
    #[instrument(level = "trace", skip(self))]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Checks if the token stream is empty.
    ///
    /// # Returns
    ///
    /// `true` if the token stream contains no tokens, `false` otherwise.
    #[instrument(level = "trace", skip(self))]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns a clone of the `Token` at the specified index.
    ///
    /// # Arguments
    ///
    /// * `idx` - The zero-based index of the token to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option<Token>` containing a clone of the token if the index is valid,
    /// otherwise `None`.
    #[instrument(skip(self))]
    pub fn get_token_at_index(&self, idx: usize) -> Option<Token> {
        if idx < self.inner.len() {
            Some(self.inner[idx].1.clone())
        } else {
            None
        }
    }
}

impl fmt::Display for TokenStream {
    /// Formats the `TokenStream` for display.
    ///
    /// This implementation reconstructs the original string from the tokens and their
    /// offsets, preserving the original whitespace between tokens.
    ///
    /// # Arguments
    ///
    /// * `f` - The formatter to write into.
    ///
    /// # Returns
    ///
    /// A `fmt::Result` indicating success or failure of the formatting operation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let words = self
            .inner
            .iter()
            .map(|(_, t)| t.to_string())
            .collect::<Vec<String>>();
        let whitespace = self
            .inner
            .iter()
            .tuple_windows()
            .map(|(first, second)| (first.0.end, second.0.start))
            .map(|t| " ".repeat(t.1 - t.0))
            .collect::<Vec<String>>();
        write!(
            f,
            "{}",
            words.iter().interleave(whitespace.iter()).join_concat()
        )
    }
}
#[cfg(test)]
mod should {
    use proptest::prelude::*;

    use crate::record::tokens::{GrokSet, Grokker, Token};

    // The below makes debugging tests much easier
    // use tracing_test::traced_test;

    prop_compose! {
        fn gen_uuid()(s in "[A-Fa-f0-9]{8}-(?:[A-Fa-f0-9]{4}-){3}[A-Fa-f0-9]{12}") -> String {
            s
        }
    }
    prop_compose! {
        fn gen_mac()(s in "(?:(?:[A-Fa-f0-9]{2}:){5}[A-Fa-f0-9]{2})") -> String {
            s
        }
    }
    prop_compose! {
        fn gen_int10()(s in "(?:[+-]?(?:[1-9]{2,3})(?:[0-9]{2,}))") -> String {
            s
        }
    }
    prop_compose! {
        fn gen_int16()(s in "(?:[+-]?(?:0x)(?:[0-9A-Fa-f]+))") -> String {
            s
        }
    }
    prop_compose! {
        fn gen_float10()(s in r"(?:[+-]?(?:(?:[0-9]+(?:\.[0-9]+))|(?:\.[0-9]+)))") -> String {
            s
        }
    }
    prop_compose! {
        fn gen_float16()(s in r"(?:[+-]?(?:0x)(?:[0-9A-Fa-f]+)(?:\.[0-9A-Fa-f]+))") -> String {
            s
        }
    }

    proptest! {
        #[test]
        fn test_token_from_parse_uuid(u in gen_uuid()) {
            let token = Token::from_parse(&u);
            prop_assert!({
                match token {
                    Token::Wildcard=>false,
                    Token::TypedMatch(Grokker::UUID)=>true,
                    Token::TypedMatch(_) => false,
                    Token::Value(_) => false,
                }
            }, "Token should be a uuid");
        }

        #[test]
        fn test_token_from_parse_mac(u in gen_mac()) {
            let token = Token::from_parse(&u);
            prop_assert!({
                match token {
                    Token::Wildcard=>false,
                    Token::TypedMatch(Grokker::MAC)=>true,
                    Token::TypedMatch(_) => false,
                    Token::Value(_) => false,
                }
            }, "Token should be a MAC address");
        }

        #[test]
        fn test_token_from_parse_int10(u in gen_int10()) {
            let token = Token::from_parse(&u);
            prop_assert!({
                match token {
                    Token::Wildcard=>false,
                    Token::TypedMatch(Grokker::Base10Integer)=>true,
                    Token::TypedMatch(_) => false,
                    Token::Value(_) => false,
                }
            }, "Token should be a base 10 integer");
        }

        #[test]
        fn test_token_from_parse_int16(u in gen_int16()) {
            let token = Token::from_parse(&u);
            prop_assert!({
                match token {
                    Token::Wildcard=>false,
                    Token::TypedMatch(Grokker::Base16Integer)=>true,
                    Token::TypedMatch(_) => false,
                    Token::Value(_) => false,
                }
            }, "Token should be a base 16 integer");
        }

        #[test]
        fn test_token_from_parse_float16(u in gen_float16()) {
            let token = Token::from_parse(&u);
            prop_assert!({
                match token {
                    Token::Wildcard=>false,
                    Token::TypedMatch(Grokker::Base16Float)=>true,
                    Token::TypedMatch(_) => false,
                    Token::Value(_) => false,
                }
            }, "Token should be a base 16 float");
        }

        #[test]
        fn test_token_from_parse_float10(u in gen_float10()) {
            let token = Token::from_parse(&u);
            prop_assert!({
                match token {
                    Token::Wildcard=>false,
                    Token::TypedMatch(Grokker::Base10Float)=>true,
                    Token::TypedMatch(_) => false,
                    Token::Value(_) => false,
                }
            }, "Token should be a base 10 float");
        }

        #[test]
        fn test_grokset_isnumeric_float10(u in gen_float10()) {
            let line = u.to_string();
            let grokset = GrokSet::new(&line);
            prop_assert!(grokset.is_numeric(), "GrokSet should indicate is_numeric");
        }

        #[test]
        fn test_grokset_isnumeric_in10(u in gen_int10()) {
            let line = u.to_string();
            let grokset = GrokSet::new(&line);
            prop_assert!(grokset.is_numeric(), "GrokSet should indicate is_numeric");
        }

        #[test]
        fn test_grokset_isnumeric_float16(u in gen_float16()) {
            let line = u.to_string();
            let grokset = GrokSet::new(&line);
            prop_assert!(grokset.is_numeric(), "GrokSet should indicate is_numeric");
        }

        #[test]
        fn test_grokset_isnumeric_int16(u in gen_int16()) {
            let line = u.to_string();
            let grokset = GrokSet::new(&line);
            prop_assert!(grokset.is_numeric(), "GrokSet should indicate is_numeric");
        }
    }
}
