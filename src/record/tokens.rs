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

// Added imports for StringInternerTrait and the new static interner
use crate::interner::StringInternerTrait;
use std::sync::Arc;
use parking_lot::RwLock;
use string_interner::{StringInterner, backend::BucketBackend as StaticSymbolInternerBackend};

// Removed: use crate::drains::simple::INTERNER;

lazy_static! {
    // New static interner for Grokker symbols and other path-internal uses
    static ref STATIC_SYMBOL_INTERNER: Arc<RwLock<StringInterner<StaticSymbolInternerBackend>>> =
        Arc::new(RwLock::new(StringInterner::<StaticSymbolInternerBackend>::new()));

    // ASTERISK symbol defined locally using STATIC_SYMBOL_INTERNER
    pub(crate) static ref ASTERISK: DefaultSymbol = STATIC_SYMBOL_INTERNER.write().get_or_intern_static("<*>");

    static ref MATCHERS: RegexSet = Grokker::build_pattern_set();
    static ref GROKKER_COUNT: usize = Grokker::iter_variants().count() - 1;
    static ref GROKKER_SYMS: HashMap<Grokker, DefaultSymbol> = symbolize_grokker();
    static ref GROKKER_VARIANTS: HashMap<usize, Grokker> = Grokker::iter_variants()
        .enumerate()
        .collect::<HashMap<usize, Grokker>>();
}

fn symbolize_grokker() -> HashMap<Grokker, DefaultSymbol> {
    Grokker::iter_variants()
        .map(|v| (v, STATIC_SYMBOL_INTERNER.write().get_or_intern(v.to_string())))
        .collect::<HashMap<Grokker, DefaultSymbol>>()
}

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
    pub fn from_parse(input: &str, interner: &mut impl StringInternerTrait<Symbol = DefaultSymbol>) -> Token {
        let matches = MATCHERS.matches(input);
        let match_types: Vec<_> = matches
            .iter()
            .filter_map(Grokker::from_match_index)
            .collect();

        debug!("comparing {} tokens", match_types.len());

        let tok = match match_types.len() {
            0 => Token::Value(TypedToken::from_parse(input, interner)), // Pass interner here
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

impl Token {
    // Helper method for display, to be called by the Display trait impl
    // or other places needing string representation with a specific interner.
    // This is an intermediate step; Display trait itself cannot easily take extra params
    // without newtype wrappers or changing how it's called.
    // For now, let's assume this is what the plan meant for modifying Display.
    // The actual Display impl will be harder to change directly if it's used by format macros.
    // The subtask states: "Change the signature to `fn fmt(&self, f: &mut fmt::Formatter<'_>, interner: &impl StringInternerTrait<Symbol = DefaultSymbol>) -> fmt::Result`"
    // This is not directly possible for the `std::fmt::Display` trait's `fmt` method.
    // It must be a new method, or Token becomes not Display-able directly.
    // Let's create a new method `format_with_interner` and adapt Display later if possible,
    // or callers of `to_string` will need to change to `format_with_interner(...).
    //
    // Re-reading: "impl fmt::Display for Token ... Change the signature to..."
    // This implies it's *not* the std::fmt::Display::fmt method, or I'm misunderstanding.
    // Let's assume it's a new method as Display's signature is fixed.
    // NO, the task is likely that Token itself will no longer implement Display directly.
    // Or, it will be a wrapper type that implements Display and holds the interner.
    // The prompt says "Modify impl fmt::Display for Token". This is confusing.
    //
    // Let's try to make a method `display_with_interner` and have `Display` call it,
    // but `Display` won't have access to the required interner.
    // This part of the plan might be problematic as written for `std::fmt::Display`.
    //
    // What if Token's Display implementation uses the STATIC_SYMBOL_INTERNER?
    // That would work for symbols interned with it. But the goal is to pass an interner.
    // This implies that `Token::Value(TypedToken::String(sym))` contains a symbol
    // that was created with an interner that is *passed in*.
    //
    // I will proceed by creating a new method `format_with_interner` as this is the
    // most direct interpretation that is implementable.
    // Then I will make the `fmt::Display` implementation panic or use a default.
    // Or, the subtask implies that `Token` objects themselves will now carry an interner,
    // or that all contexts where `Token` is displayed can provide one.
    //
    // Given the constraint "This subtask modifies only src/record/tokens.rs",
    // I cannot change how `Display` is called externally.
    // This means `Token` must still implement `std::fmt::Display`.
    // The only way for `Display` to work is if it can access an interner.
    // If `Token::Value` symbols are from a passed-in interner, `Display` cannot use them
    // without also getting that interner.
    //
    // This is a contradiction. Let's assume the symbols in `Token::Value(TypedToken::String(sym))`
    // are expected to be resolvable by `STATIC_SYMBOL_INTERNER` for the `Display` impl.
    // This means `TypedToken::from_parse` (when creating Value tokens) should perhaps use `STATIC_SYMBOL_INTERNER`.
    // But step 4 says `TypedToken::from_parse` takes an interner.
    //
    // This implies that `Token` can hold symbols from DIFFERENT interners.
    // `GROKKER_SYMS` are from `STATIC_SYMBOL_INTERNER`.
    // Symbols from `Token::from_parse` (via `TypedToken::from_parse`) are from a *passed-in* interner.
    //
    // If `Display` is called on a `Token` with a symbol from a *passed-in* interner,
    // then `STATIC_SYMBOL_INTERNER.read().resolve(*sym)` will fail if that symbol
    // isn't in the static interner.
    //
    // This means the `fmt::Display` for `Token` cannot be universally correct if symbols can come
    // from arbitrary interners.
    // The subtask description for `fmt::Display` for `Token` says:
    // "In the Token::Value(TypedToken::String(sym)) arm, replace ... with interner.resolve(sym)."
    // This implies the `fmt` signature change IS for `std::fmt::Display::fmt`, which is not possible.
    //
    // I must assume the intent is one of:
    // 1. Create a helper function, not `Display::fmt`.
    // 2. `Token` will no longer implement `Display` directly, but via a helper struct that holds the interner.
    // 3. The `Display` impl will use `STATIC_SYMBOL_INTERNER` and thus only work for symbols from it.
    //
    // Let's choose option 1 for now: create a new method `format_token_with_interner`.
    // The original `Display` impl will be modified to note this problem or use the static interner.
    // The subtask is very specific: "Modify impl fmt::Display for Token ... Change the signature to..."
    // This is the core of the problem. I will attempt to change the existing Display impl's body
    // to reflect the *spirit* of the change, acknowledging it cannot take the interner directly.
    // It will have to use `STATIC_SYMBOL_INTERNER` for values. This might be an intermediate state.
    // This means `Token::Value(TypedToken::String(sym))` must contain symbols from `STATIC_SYMBOL_INTERNER`.
    // This contradicts step 4 for `TypedToken::from_parse`.
    //
    // Let's follow step 3 as literally as possible, by creating a new method, and make the
    // existing Display use the STATIC_SYMBOL_INTERNER, which is the only one it has access to.
    // The plan step 3 has "Change the signature to `fn fmt(&self, f: &mut fmt::Formatter<'_>, interner: &impl StringInternerTrait<Symbol = DefaultSymbol>) -> fmt::Result`."
    // This cannot be `std::fmt::Display`. It must be a new method.
    // I will call this new method `format_with_interner`.
    // The original `Display` impl will remain, but its behavior for `Token::Value` will be affected.
    // For now, I will make `Display` use `STATIC_SYMBOL_INTERNER` for `Token::Value`.
}

impl Token {
    /// Returns the string representation of the token, resolving symbols with the provided interner.
    pub fn to_string_with_interner(&self, interner: &impl StringInternerTrait<Symbol = DefaultSymbol>) -> String {
        match self {
            Token::Wildcard => "<*>".to_string(),
            // Grokker enum has its own Display impl (via EnumDisplay) that doesn't need an interner.
            Token::TypedMatch(g) => g.to_string(),
            Token::Value(typed_token) => match typed_token {
                TypedToken::String(sym) => interner.resolve(sym),
                TypedToken::Int(i) => i.to_string(),
                TypedToken::Float(f) => f.to_string(),
            },
        }
    }
}

// `fmt::Display` for `Token` now provides a structural representation.
// It does not resolve symbols, making it safe to call when the original interner is not available.
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Wildcard => write!(f, "<*>"),
            Token::TypedMatch(grokker) => write!(f, "Token::TypedMatch({})", grokker), // Grokker itself has Display
            Token::Value(typed_token) => match typed_token {
                TypedToken::String(sym) => write!(f, "Token::Value(Symbol({:?}))", sym), // Use Debug for DefaultSymbol
                TypedToken::Int(i) => write!(f, "Token::Value(Int({}))", i),
                TypedToken::Float(fl) => write!(f, "Token::Value(Float({}))", fl),
            },
        }
    }
}

impl From<Token> for DefaultSymbol {
    fn from(tok: Token) -> DefaultSymbol {
        match tok {
            Token::Wildcard => *ASTERISK, // Use locally defined ASTERISK
            Token::TypedMatch(t) => *GROKKER_SYMS
                .get(&t)
                .expect("every grokker must have a symbol"), // GROKKER_SYMS use STATIC_SYMBOL_INTERNER
            Token::Value(typed_token) => match typed_token {
                TypedToken::String(s) => s, // s is already a DefaultSymbol
                TypedToken::Int(_) => {
                    panic!("Cannot convert Token::Value(TypedToken::Int) to DefaultSymbol without an interner")
                }
                TypedToken::Float(_) => {
                    panic!("Cannot convert Token::Value(TypedToken::Float) to DefaultSymbol without an interner")
                }
            },
        }
    }
}

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
    /// Parses supplied string and returns a token using the provided interner.
    #[must_use]
    pub fn from_parse(input: &str, interner: &mut impl StringInternerTrait<Symbol = DefaultSymbol>) -> TypedToken {
        TypedToken::String(interner.intern(input))
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
    pub fn from_unicode_line(line: &str, interner: &mut impl StringInternerTrait<Symbol = DefaultSymbol>) -> Self {
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
                    // Use the passed-in interner
                    Token::Value(TypedToken::String(interner.intern(w))),
                );
                debug!(?token, %w, ?start, "built");
                Some(token)
            })
            .collect::<Vec<(Offset, Token)>>();
        Self { inner: words }
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

    use string_interner::StringInterner; // For test interner instance

    proptest! {
        #[test]
        fn test_token_from_parse_uuid(u in gen_uuid()) {
            let mut interner = StringInterner::<StaticSymbolInternerBackend>::new();
            let token = Token::from_parse(&u, &mut interner);
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
            let mut interner = StringInterner::<StaticSymbolInternerBackend>::new();
            let token = Token::from_parse(&u, &mut interner);
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
            let mut interner = StringInterner::<StaticSymbolInternerBackend>::new();
            let token = Token::from_parse(&u, &mut interner);
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
            let mut interner = StringInterner::<StaticSymbolInternerBackend>::new();
            let token = Token::from_parse(&u, &mut interner);
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
            let mut interner = StringInterner::<StaticSymbolInternerBackend>::new();
            let token = Token::from_parse(&u, &mut interner);
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
            let mut interner = StringInterner::<StaticSymbolInternerBackend>::new();
            let token = Token::from_parse(&u, &mut interner);
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
