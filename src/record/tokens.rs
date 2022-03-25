use std::{collections::HashMap, fmt};

use crate::INTERNER;
use anyhow::Error;
use float_eq::float_eq;
use itertools::Itertools;
use joinery::JoinableIterator;
use lazy_static::lazy_static;
use regex::RegexSet;
use string_interner::DefaultSymbol;

use super::ASTERISK;

lazy_static! {
    static ref MATCHERS: RegexSet = Grokker::build_pattern_set();
    static ref GROKKER_COUNT: usize = Grokker::iter_variants().count() - 1;
    static ref GROKKER_SYMS: HashMap<Grokker, DefaultSymbol> = symbolize_grokker();
}

fn symbolize_grokker() -> HashMap<Grokker, DefaultSymbol> {
    Grokker::iter_variants()
        .map(|v| (v, INTERNER.write().get_or_intern(&v.to_string())))
        .collect::<HashMap<Grokker, DefaultSymbol>>()
}

custom_derive! {
    #[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, IterVariants(GrokkerVariants), EnumDisplay)]
    pub enum Grokker {
        Base10Integer,
        Base10Float,
        Base16Integer,
        Base16Float,
        QuotedString,
        Word,
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
    fn to_pattern(&self) -> String {
        match self {
            Grokker::Base10Integer => r"(?:[+-]?(?:[0-9]+))".to_string(),
            Grokker::Base10Float => {
                r"(?<![0-9.+-])(?>[+-]?(?:(?:[0-9]+(?:\.[0-9]+)?)|(?:\.[0-9]+)))".to_string()
            }
            Grokker::Base16Integer => r"(?<![0-9A-Fa-f])(?:[+-]?(?:0x)?(?:[0-9A-Fa-f]+))".to_string(),
            Grokker::Base16Float => {
                r"\b(?<![0-9A-Fa-f.])(?:[+-]?(?:0x)?(?:(?:[0-9A-Fa-f]+(?:\.[0-9A-Fa-f]*)?)|(?:\.[0-9A-Fa-f]+)))\b".to_string()
            }
            Grokker::QuotedString => {
                r#"(?>(?<!\\)(?>"(?>\\.|[^\\"]+)+"|""|(?>'(?>\\.|[^\\']+)+')|''|(?>`(?>\\.|[^\\`]+)+`)|``))"#.to_string()
            }
            Grokker::Word => r"\b\w+\b".to_string(),
            Grokker::UUID => r"[A-Fa-f0-9]{8}-(?:[A-Fa-f0-9]{4}-){3}[A-Fa-f0-9]{12}".to_string(),
            Grokker::MAC => r"(?:(?:[A-Fa-f0-9]{2}:){5}[A-Fa-f0-9]{2})".to_string(),
            Grokker::IPv6 => {
                r"((([0-9A-Fa-f]{1,4}:){7}([0-9A-Fa-f]{1,4}|:))|(([0-9A-Fa-f]{1,4}:){6}(:[0-9A-Fa-f]{1,4}|((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){5}(((:[0-9A-Fa-f]{1,4}){1,2})|:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){4}(((:[0-9A-Fa-f]{1,4}){1,3})|((:[0-9A-Fa-f]{1,4})?:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){3}(((:[0-9A-Fa-f]{1,4}){1,4})|((:[0-9A-Fa-f]{1,4}){0,2}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){2}(((:[0-9A-Fa-f]{1,4}){1,5})|((:[0-9A-Fa-f]{1,4}){0,3}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){1}(((:[0-9A-Fa-f]{1,4}){1,6})|((:[0-9A-Fa-f]{1,4}){0,4}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(:(((:[0-9A-Fa-f]{1,4}){1,7})|((:[0-9A-Fa-f]{1,4}){0,5}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:)))(%.+)?".to_string()
            }
            Grokker::IPv4 => {
                r"(?<![0-9])(?:(?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])[.](?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])[.](?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5])[.](?:[0-1]?[0-9]{1,2}|2[0-4][0-9]|25[0-5]))(?![0-9])".to_string()
            }
            Grokker::Hostname => {
                r"\b(?:[0-9A-Za-z][0-9A-Za-z-]{0,62})(?:\.(?:[0-9A-Za-z][0-9A-Za-z-]{0,62}))*(\.?|\b)".to_string()
            }
            Grokker::Month => {
                r"\b(?:[Jj]an(?:uary|uar)?|[Ff]eb(?:ruary|ruar)?|[Mm](?:a|ä)?r(?:ch|z)?|[Aa]pr(?:il)?|[Mm]a(?:y|i)?|[Jj]un(?:e|i)?|[Jj]ul(?:y)?|[Aa]ug(?:ust)?|[Ss]ep(?:tember)?|[Oo](?:c|k)?t(?:ober)?|[Nn]ov(?:ember)?|[Dd]e(?:c|z)(?:ember)?)\b".to_string()
            }
            Grokker::Day => {
                r"(?:Mon(?:day)?|Tue(?:sday)?|Wed(?:nesday)?|Thu(?:rsday)?|Fri(?:day)?|Sat(?:urday)?|Sun(?:day)?)".to_string()
            }
        }
    }

    fn build_pattern_set() -> RegexSet {
        let variants = Grokker::iter_variants()
            .map(|v| v.to_pattern())
            .collect::<Vec<String>>();
        RegexSet::new(variants).expect("valid regular expressions compile")
    }

    pub fn from_match_index(idx: usize) -> Option<Grokker> {
        if idx > *GROKKER_COUNT {
            return None;
        }
        Some(Grokker::iter_variants().collect::<Vec<Grokker>>()[idx])
    }
}

#[derive(Debug, Clone)]
pub enum Token {
    /// Token that matches any other token
    Wildcard,
    /// Token that matches any value of the inner type
    TypedMatch(Grokker),
    /// Token containing a typed, non-wildcard value
    Value(TypedToken),
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let out: String = match self {
            Token::Wildcard => "*".to_string(),
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
    pub fn from_parse(input: &str) -> Result<TypedToken, Error> {
        let tok = INTERNER.write().get_or_intern(input);
        Ok(TypedToken::String(tok))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Offset {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenStream {
    inner: Vec<(Offset, Token)>,
}

impl TokenStream {
    pub fn from_line(line: &str) -> Self {
        let mut interner = INTERNER.write();
        let tokens = line
            .char_indices()
            .filter(|t| {
                (t.0 == 0 && !t.1.is_whitespace()) // The very first char needs special handling
                || (t.1.is_whitespace()
                    && line
                        .chars()
                        .clone()
                        .nth(t.0 - 1).map(|c| !c.is_whitespace())
                        .unwrap())
            })
            .map(|t| t.0)
            .chain(vec![line.len()].into_iter())
            .tuple_windows::<(_, _)>()
            .map(|t| (if t.0 == 0 { t.0 } else { t.0 + 1 }, t.1))
            .map(|t| {
                (
                    Offset {
                        start: t.0,
                        end: t.1,
                    },
                    Token::Value(TypedToken::String(
                        interner.get_or_intern(line.get(t.0..t.1).unwrap()),
                    )),
                )
            })
            .collect::<Vec<(Offset, Token)>>();
        Self { inner: tokens }
    }

    pub fn first(&self) -> Option<Token> {
        match self.inner.len() {
            0 => None,
            _ => Some(self.inner[0].1.clone()),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn get_token_at_index(&self, idx: usize) -> Option<Token> {
        match idx < self.inner.len() {
            true => Some(self.inner[idx].1.clone()),
            false => None,
        }
    }
}

impl fmt::Display for TokenStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        //TODO: make use of offset to correctly restore whitespace
        let disp = self
            .inner
            .iter()
            .map(|(_offset, token)| token)
            .join_with(" ");
        write!(f, "{}", disp)
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Token::Wildcard => true,
            Token::TypedMatch(tm) => match other {
                Token::Wildcard => true,
                Token::TypedMatch(otm) => tm == otm,
                Token::Value(_) => false,
            },
            Token::Value(val) => match other {
                Token::Wildcard => true,
                Token::TypedMatch(_) => match val {
                    TypedToken::String(_) => false,
                    TypedToken::Int(_) => false,
                    TypedToken::Float(_) => false,
                },
                Token::Value(other_val) => match val {
                    TypedToken::String(string_val) => {
                        if let TypedToken::String(other_string) = other_val {
                            return string_val == other_string;
                        }
                        false
                    }
                    TypedToken::Int(int_val) => {
                        if let TypedToken::Int(other_int) = other_val {
                            return int_val == other_int;
                        }
                        false
                    }
                    TypedToken::Float(float_val) => {
                        if let TypedToken::Float(other_float) = other_val {
                            return float_eq!(float_val, other_float, ulps <= 1);
                        }
                        false
                    }
                },
            },
        }
    }
}

impl Eq for Token {}

#[cfg(test)]
mod should {
    use crate::record::tokens::{Token, TypedToken};
    use crate::INTERNER;
    use proptest::prelude::*;
    use spectral::prelude::*;

    #[test]
    fn test_wildcard_lhs() {
        let lhs = Token::Wildcard;
        let rhs = Token::Value(TypedToken::String(INTERNER.write().get_or_intern("foo")));
        assert_that(&lhs).is_equal_to(rhs.clone());
        assert_that(&rhs).is_equal_to(lhs);
    }

    proptest! {
        #[test]
        fn test_wildcard_matches_any_string(s in "\\PC*") {
            let wildcard = Token::Wildcard;
            let val = Token::Value(TypedToken::String(INTERNER.write().get_or_intern(s)));
            assert_that(&wildcard).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(wildcard);
        }

        #[test]
        fn test_wildcard_matches_any_int(s in i64::MIN..i64::MAX) {
            let wildcard = Token::Wildcard;
            let val = Token::Value(TypedToken::Int(s));
            assert_that(&wildcard).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(wildcard);
        }

        #[test]
        fn test_wildcard_matches_any_positive_float(s in 0f64..f64::MAX) {
            let wildcard = Token::Wildcard;
            let val = Token::Value(TypedToken::Float(s));
            assert_that(&wildcard).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(wildcard);
        }

        #[test]
        fn test_wildcard_matches_any_negative_float(s in f64::MIN..0f64) {
            let wildcard = Token::Wildcard;
            let val = Token::Value(TypedToken::Float(s));
            assert_that(&wildcard).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(wildcard);
        }

        #[test]
        fn test_value_string_matches_same_string(s in "\\PC*") {
            let val1 = Token::Value(TypedToken::String(INTERNER.write().get_or_intern(s.clone())));
            let val2 = Token::Value(TypedToken::String(INTERNER.write().get_or_intern(s)));
            assert_that(&val1).is_equal_to(val2.clone());
            assert_that(&val2).is_equal_to(val1);
        }

        #[test]
        fn test_value_int_matches_same_int(s in i64::MIN..i64::MAX) {
            let val1 = Token::Value(TypedToken::Int(s));
            let val2 = Token::Value(TypedToken::Int(s));
            assert_that(&val1).is_equal_to(val2.clone());
            assert_that(&val2).is_equal_to(val1);
        }

        #[test]
        fn test_value_float_matches_same_positive_float(s in 0f64..f64::MAX) {
            let val1 = Token::Value(TypedToken::Float(s));
            let val2 = Token::Value(TypedToken::Float(s));
            assert_that(&val1).is_equal_to(val2.clone());
            assert_that(&val2).is_equal_to(val1);
        }

        #[test]
        fn test_value_float_matches_same_negative_float(s in f64::MIN..0f64) {
            let val1 = Token::Value(TypedToken::Float(s));
            let val2 = Token::Value(TypedToken::Float(s));
            assert_that(&val1).is_equal_to(val2.clone());
            assert_that(&val2).is_equal_to(val1);
        }
    }
}
