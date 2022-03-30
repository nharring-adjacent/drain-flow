pub mod tokens;
extern crate derive_more;

use std::fmt;

use crate::INTERNER;

use lazy_static::lazy_static;

use rksuid::rksuid;
use string_interner::DefaultSymbol;
use tracing::{debug, instrument};

use self::tokens::{Token, TokenStream, TypedToken};

lazy_static! {
    static ref ASTERISK: DefaultSymbol = INTERNER.write().get_or_intern_static("*");
}
#[derive(Clone, Debug)]
pub struct Record {
    inner: TokenStream,
    pub uid: rksuid::Ksuid,
}
impl Record {
    #[instrument]
    pub fn new(line: String) -> Self {
        Self {
            inner: TokenStream::from_unicode_line(&line),
            uid: rksuid::new(None, None),
        }
    }

    // #[instrument]
    pub fn calc_sim_score(&self, candidate: &Record) -> u64 {
        let pairs = self
            .into_iter()
            .zip(candidate.into_iter())
            .collect::<Vec<(_, _)>>();
        // let mut interner = INTERNER.write();
        let score = pairs
            .iter()
            .filter(|(this, other)| {
                if this == other {
                    debug!("{}", format!("found match of {} and {}\n", this, other));
                    true
                } else {
                    false
                }
            })
            .fold(0_u64, |acc, _pair| acc + 1);
        score
    }

    #[instrument]
    pub fn first(&self) -> Option<DefaultSymbol> {
        self.inner.first().map(|f| f.into())
    }

    #[instrument]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    #[instrument]
    pub fn resolve(sym: DefaultSymbol) -> Option<String> {
        INTERNER.read().resolve(sym).map(|s| s.to_owned())
    }
}

pub struct RecordIntoIter {
    record: Record,
    index: usize,
}

pub struct RecordRefIterator<'a> {
    record: &'a Record,
    index: usize,
}

impl Iterator for RecordIntoIter {
    type Item = String;
    fn next(&mut self) -> Option<String> {
        if self.index >= self.record.len() {
            return None;
        }
        let sym = match self.record.inner.get_token_at_index(self.index) {
            Some(t) => match t {
                tokens::Token::Wildcard => "*".to_string(),
                tokens::Token::TypedMatch(t) => format!("{}", t),
                tokens::Token::Value(v) => match v {
                    TypedToken::String(sym) => INTERNER
                        .read()
                        .resolve(sym)
                        .expect("symbol failed to resolve")
                        .to_owned(),
                    TypedToken::Int(i) => i.to_string(),
                    TypedToken::Float(f) => f.to_string(),
                },
            },
            None => todo!(),
        };

        self.index += 1;
        Some(sym)
    }
}

impl IntoIterator for Record {
    type Item = String;
    type IntoIter = RecordIntoIter;
    fn into_iter(self) -> Self::IntoIter {
        RecordIntoIter {
            record: self,
            index: 0,
        }
    }
}
impl<'a> Iterator for RecordRefIterator<'a> {
    type Item = Token;
    fn next(&mut self) -> Option<Token> {
        if let Some(val) = self.record.inner.get_token_at_index(self.index) {
            self.index += 1;
            return Some(val);
        }
        None
    }
}
impl<'a> IntoIterator for &'a Record {
    type Item = Token;
    type IntoIter = RecordRefIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        RecordRefIterator {
            record: self,
            index: 0,
        }
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}
#[cfg(test)]
mod should {
    use crate::Record;
    use crate::INTERNER;
    use joinery::{Joinable, JoinableIterator};
    use proptest::prelude::*;
    use proptest::string::string_regex;
    use spectral::prelude::*;

    prop_compose! {
        fn gen_word()(s in "[[:alpha:]]+") -> String {
            s
        }
    }

    fn gen_variable_string() -> impl Strategy<Value = String> {
        prop_oneof![
            // UUID
            string_regex(r"[A-Fa-f0-9]{8}-(?:[A-Fa-f0-9]{4}-){3}[A-Fa-f0-9]{12}").unwrap(),
            // MAC address
            string_regex(r"(?:(?:[A-Fa-f0-9]{2}:){5}[A-Fa-f0-9]{2})").unwrap(),
            // IPv6
            string_regex(r"((([0-9A-Fa-f]{1,4}:){7}([0-9A-Fa-f]{1,4}|:))|(([0-9A-Fa-f]{1,4}:){6}(:[0-9A-Fa-f]{1,4}|((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){5}(((:[0-9A-Fa-f]{1,4}){1,2})|:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3})|:))|(([0-9A-Fa-f]{1,4}:){4}(((:[0-9A-Fa-f]{1,4}){1,3})|((:[0-9A-Fa-f]{1,4})?:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){3}(((:[0-9A-Fa-f]{1,4}){1,4})|((:[0-9A-Fa-f]{1,4}){0,2}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){2}(((:[0-9A-Fa-f]{1,4}){1,5})|((:[0-9A-Fa-f]{1,4}){0,3}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(([0-9A-Fa-f]{1,4}:){1}(((:[0-9A-Fa-f]{1,4}){1,6})|((:[0-9A-Fa-f]{1,4}){0,4}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:))|(:(((:[0-9A-Fa-f]{1,4}){1,7})|((:[0-9A-Fa-f]{1,4}){0,5}:((25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)(\.(25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)){3}))|:)))").unwrap(),
            // Base 10 Integer
            string_regex(r"(?:[+-]?(?:[0-9]+))").unwrap(),
            // Months, too flaky to use yet becasue not enough possible values
            // string_regex(r"(?:[Jj]an(?:uary|uar)?|[Ff]eb(?:ruary|ruar)?|[Mm](?:a|ä)?r(?:ch|z)?|[Aa]pr(?:il)?|[Mm]a(?:y|i)?|[Jj]un(?:e|i)?|[Jj]ul(?:y)?|[Aa]ug(?:ust)?|[Ss]ep(?:tember)?|[Oo](?:c|k)?t(?:ober)?|[Nn]ov(?:ember)?|[Dd]e(?:c|z)(?:ember)?)").unwrap(),
        ]
    }

    fn gen_phrase(len: usize) -> impl Strategy<Value = String> {
        prop::collection::vec(gen_word(), len)
            .prop_flat_map(|vec| Just(vec.iter().join_with(" ").to_string()))
    }

    fn gen_vars(len: usize) -> impl Strategy<Value = String> {
        prop::collection::vec(gen_variable_string(), len)
            .prop_flat_map(|vec| Just(vec.iter().join_with(" ").to_string()))
    }

    fn gen_complex(base: usize, variable: usize) -> impl Strategy<Value = String> {
        let base = gen_phrase(base);
        let vars = gen_vars(variable);
        (base, vars).prop_map(|(b, v)| [b, v].join_with(" ").to_string())
    }

    prop_compose! {
        fn gen_matching_lines(base_len: usize, var_count: usize, num_lines: usize)(base_phrase in gen_phrase(base_len), var_set in prop::collection::vec(gen_vars(var_count), num_lines)) -> Vec<String> {
            var_set.iter().map(|v| {[base_phrase.clone(), v.to_string()].join_with(" ").to_string()}).collect::<Vec<String>>()
        }
    }

    proptest! {
        #[test]
        fn test_proptest_base_record_new(phrase in gen_phrase(5)) {
            let rec = Record::new(phrase.clone());
            prop_assert_eq!(phrase, rec.to_string());
        }
    }

    proptest! {
        #[test]
        fn test_proptest_variable_record_new(line in gen_complex(7, 3)) {
            // Because we don't try to fully preserve whitespace semantics
            // instead we test that the stringified form of the record is "stable"
            let rec = Record::new(line.clone());
            let rec2 = Record::new(rec.to_string());
            prop_assert_eq!(rec.to_string(), rec2.to_string());

            // Whitespace internally is preserved, only the end is missing
            let reconstituted = rec.to_string();
            prop_assert!(line.contains(&reconstituted));
        }
    }

    proptest! {
        #[test]
        fn test_matching_records(lines in gen_matching_lines(7, 3, 3)) {
            let recs = lines.iter().map(|l| Record::new(l.clone())).collect::<Vec<Record>>();
            let base = recs[0].clone();
            let score1 = base.calc_sim_score(&recs[1].clone());
            let score2 = base.calc_sim_score(&recs[2].clone());
            assert_eq!(score1, score2);
            assert_eq!(score1, 7);
        }
    }

    #[test]
    fn test_record_first() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        let val = rec.first().unwrap();
        assert_eq!(INTERNER.read().resolve(val).unwrap(), "Message");
    }

    #[test]
    fn test_record_len() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        assert_eq!(rec.len(), 7);
    }

    #[test]
    fn test_consuming_iter() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input.clone());
        let tokens = rec.into_iter().collect::<Vec<String>>();
        let words = &input
            .split(|c: char| c.is_whitespace())
            .map(|s| s.to_owned())
            .collect::<Vec<String>>();
        assert_that(&tokens.iter()).contains_all_of(&words.iter());
    }

    #[test]
    fn test_non_consuming_iter() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        let tokens = (&rec).into_iter().collect::<Vec<_>>();
        assert_that(&tokens).has_length(7);
    }
}
