pub mod tokens;
extern crate derive_more;

use std::{fmt};

use crate::INTERNER;

use lazy_static::lazy_static;

use rksuid::rksuid;
use string_interner::{DefaultSymbol};
use tracing::{instrument};

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
            inner: TokenStream::from_line(&line),
            uid: rksuid::new(None, None),
        }
    }

    #[instrument]
    pub fn calc_sim_score(&self, candidate: &Record) -> u64 {
        let pairs = self
            .into_iter()
            .zip(candidate.into_iter())
            .collect::<Vec<(_, _)>>();
        // let mut interner = INTERNER.write();
        let score = pairs
            .iter()
            .filter(|(this, other)| this == other)
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
    use spectral::prelude::*;
    use tracing_test::traced_test;

    #[traced_test]
    #[test]
    fn test_record_new() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input.clone());
        assert_eq!(input, rec.to_string());
    }

    #[traced_test]
    #[test]
    fn test_record_calc_sim_score() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let input2 = "Message send succeeded with flying colors".to_string();
        let rec = Record::new(input);
        let other = Record::new(input2);
        let score = rec.calc_sim_score(&other);
        assert_eq!(score, 2);
    }

    #[traced_test]
    #[test]
    fn test_record_first() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        let val = rec.first().unwrap();
        assert_eq!(INTERNER.read().resolve(val).unwrap(), "Message");
    }

    #[traced_test]
    #[test]
    fn test_record_len() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        assert_eq!(rec.len(), 7);
    }

    #[traced_test]
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

    #[traced_test]
    #[test]
    fn test_non_consuming_iter() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        let tokens = (&rec).into_iter().collect::<Vec<_>>();
        assert_that(&tokens).has_length(7);
    }
}
