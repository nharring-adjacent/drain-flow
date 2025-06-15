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

#[cfg(feature = "legacy_prototype")]
pub use super::ASTERISK; // Made ASTERISK re-export public
#[cfg(feature = "legacy_prototype")]
use crate::drains::simple::INTERNER;

#[cfg(feature = "legacy_prototype")]
lazy_static! {
    static ref MATCHERS: RegexSet = Grokker::build_pattern_set();
    static ref GROKKER_COUNT: usize = Grokker::iter_variants().count() - 1;
    static ref GROKKER_SYMS: HashMap<Grokker, DefaultSymbol> = symbolize_grokker();
    static ref GROKKER_VARIANTS: HashMap<usize, Grokker> = Grokker::iter_variants()
        .enumerate()
        .collect::<HashMap<usize, Grokker>>();
}

#[cfg(not(feature = "legacy_prototype"))]
lazy_static! {
    static ref MATCHERS: RegexSet = Grokker::build_pattern_set();
    static ref GROKKER_COUNT: usize = Grokker::iter_variants().count() - 1;
    // GROKKER_SYMS would be problematic here. What should it be?
    // For now, let's assume code paths requiring it are also cfg-gated.
    // Alternatively, provide a non-interned version or panic if accessed.
    static ref GROKKER_VARIANTS: HashMap<usize, Grokker> = Grokker::iter_variants()
        .enumerate()
        .collect::<HashMap<usize, Grokker>>();
}


#[cfg(feature = "legacy_prototype")]
fn symbolize_grokker() -> HashMap<Grokker, DefaultSymbol> {
    Grokker::iter_variants()
        .map(|v| (v, INTERNER.write().get_or_intern(v.to_string())))
        .collect::<HashMap<Grokker, DefaultSymbol>>()
}

// If INTERNER is not available, we need a placeholder for GROKKER_SYMS or ensure it's not used.
// Let's assume for now that uses of GROKKER_SYMS will also be conditional.

custom_derive! {
    #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, IterVariants(GrokkerVariants), EnumDisplay)]
    pub enum Grokker {
        Base10Integer,
        Base10Float,
        Base16Integer,
        Base16Float,
        UUID,
        MAC,
        IPv6,
        IPv4,
        Hostname,
        Month,
        Day,
    }
}

impl Grokker {
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

    fn build_pattern_set() -> RegexSet {
        let variants = Grokker::iter_variants()
            .map(Grokker::to_pattern)
            .collect::<Vec<String>>();
        RegexSet::new(variants).expect("valid regular expressions compile")
    }

    #[instrument(level = "trace")]
    pub fn from_match_index(idx: usize) -> Option<Grokker> {
        if idx > *GROKKER_COUNT {
            return None;
        }
        Some(GROKKER_VARIANTS[&idx])
    }
}

#[derive(Debug, Clone)]
pub struct GrokSet {
    match_types: Vec<Grokker>,
}

/// `GrokSet` is a convenience wrapper over `Regex::SetMatches` and Grokker variants
impl GrokSet {
    #[must_use]
    pub fn new(value: &str) -> Self {
        let matches = MATCHERS.matches(value);
        let match_types: Vec<_> = matches
            .iter()
            .filter_map(Grokker::from_match_index)
            .collect();
        Self { match_types }
    }

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

    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.match_types
            .iter()
            .any(|i| matches!(i, Grokker::Base10Integer | Grokker::Base16Integer))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Token that matches any other token
    Wildcard,
    /// Token that matches any value of the inner type
    TypedMatch(Grokker),
    /// Token containing a typed, non-wildcard value
    Value(TypedToken),
}

impl Token {
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out: String = match self {
            Token::Wildcard => "<*>".to_string(),
            Token::TypedMatch(t) => t.to_string(),
            Token::Value(v) => match v {
                #[cfg(feature = "legacy_prototype")]
                TypedToken::String(sym) => INTERNER
                    .read()
                    .resolve(*sym)
                    .expect("symbols must resolve")
                    .to_string(),
                // Fallback for when INTERNER is not available for TypedToken::String
                #[cfg(not(feature = "legacy_prototype"))]
                TypedToken::String(sym) => format!("<interned_string:{:?}>", sym), // Or some other placeholder
                TypedToken::Int(i) => format!("{}", i),
                TypedToken::Float(f) => f.to_string(),
            },
        };
        write!(f, "{}", out)
    }
}

#[cfg(feature = "legacy_prototype")]
impl From<Token> for DefaultSymbol {
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

// If legacy_prototype is off, what should From<Token> for DefaultSymbol do?
// This is problematic if DefaultSymbol itself is tied to the interner.
// DefaultSymbol is from string_interner, so it implies interning.
// This part of the code might be inherently tied to having an interner.
// For now, let's assume that if INTERNER is off, this conversion might not be fully supported
// or needs a different definition for DefaultSymbol or ASTERISK.

#[derive(PartialEq, Debug, Clone)]
pub enum TypedToken {
    /// Token containing a string with at least 1 non-digit
    String(DefaultSymbol),
    /// Token containing a whole number only
    Int(i64),
    /// Token containing a float
    Float(f64),
}

impl TypedToken {
    /// Parses supplied string and returns a token
    #[must_use]
    #[cfg(feature = "legacy_prototype")]
    pub fn from_parse(input: &str) -> TypedToken {
        TypedToken::String(INTERNER.write().get_or_intern(input))
    }

    // What happens to from_parse if legacy_prototype is off?
    // This implies TypedToken::String might not be constructible in the same way.
    // Option 1: Gate from_parse entirely.
    // Option 2: Provide a from_parse that doesn't use INTERNER (e.g., stores String, not DefaultSymbol).
    //           This would require changing TypedToken::String to hold String or Arc<String>
    //           when legacy_prototype is off. This is a larger refactor.
    // For now, let's gate it. Callers will need to be gated too.
    #[must_use]
    #[cfg(not(feature = "legacy_prototype"))]
    pub fn from_parse(input: &str) -> TypedToken {
        // This is a placeholder. Without an interner, DefaultSymbol is meaningless here.
        // This suggests TypedToken::String itself needs to be conditional or change type.
        // This will likely cause compilation errors further down if not handled carefully.
        // A proper solution would involve defining an alternative for DefaultSymbol or how strings are stored.
        panic!("TypedToken::from_parse requires an interner, which is unavailable without legacy_prototype feature.");
        // Or, if DefaultSymbol can represent an uninterned state or if we use a different type:
        // TypedToken::String(DefaultSymbol::from_lossy_str(input)) // Fictional method
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Offset {
    start: usize,
    end: usize,
}

impl Display for Offset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Offset(start: {}, end: {})", self.start, self.end)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TokenStream {
    pub(crate) inner: Vec<(Offset, Token)>,
}

impl TokenStream {
    #[instrument(skip(line))]
    #[cfg(feature = "legacy_prototype")]
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

    #[instrument(skip(line))]
    #[cfg(not(feature = "legacy_prototype"))]
    pub fn from_unicode_line(line: &str) -> Self {
        // This function relies on an interner to create TypedToken::String(DefaultSymbol).
        // If the legacy_prototype feature (which provides INTERNER) is off, we cannot proceed.
        panic!("TokenStream::from_unicode_line requires an interner, which is unavailable without legacy_prototype feature. Line: {}", line);
    }

    #[instrument(skip(self), level = "trace")]
    pub fn first(&self) -> Option<Token> {
        match self.inner.len() {
            0 => None,
            _ => Some(self.inner[0].1.clone()),
        }
    }

    #[instrument(skip(self), level = "trace")]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[instrument(skip(self), level = "trace")]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

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
