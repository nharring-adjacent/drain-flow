pub mod tokens;
extern crate derive_more;

use std::fmt;
use std::sync::RwLock;

use itertools::Itertools;
use lazy_static::lazy_static;
use rksuid::rksuid;
use string_interner::{DefaultSymbol, StringInterner};
use tracing::{info, instrument};

lazy_static! {
    static ref INTERNER: RwLock<StringInterner> = RwLock::new(StringInterner::default());
}
#[derive(Clone, Debug)]
pub struct Record {
    interner: &'static RwLock<StringInterner>,
    pub raw_message: DefaultSymbol,
    tokens: Vec<DefaultSymbol>,
    // pos: usize,
    pub uid: rksuid::Ksuid,
}
impl Record {
    pub fn new(line: String) -> Self {
        info!(%line, "new record from");
        let mut interner = INTERNER.write().expect("lock is valid");
        Self {
            interner: &INTERNER,
            raw_message: interner.get_or_intern(line.clone()),
            tokens: line
                .as_str()
                .char_indices()
                .filter(|t| {
                    (t.0 == 0 && !t.1.is_whitespace()) // The very first char needs special handling
                        || (t.1.is_whitespace()
                            && line
                                .chars()
                                .clone()
                                .nth(t.0 - 1)
                                .and_then(|c| Some(!c.is_whitespace()))
                                .unwrap())
                })
                .map(|t| t.0)
                .chain(vec![line.len()].into_iter())
                .tuple_windows::<(_, _)>()
                .map(|t| (if t.0 == 0 { t.0 } else { t.0 + 1 }, t.1))
                .map(|t| interner.get_or_intern(line.as_str().get(t.0..t.1).unwrap().to_owned()))
                .collect::<Vec<DefaultSymbol>>(),
            // pos: 0,
            uid: rksuid::new(None, None),
        }
    }

    pub fn calc_sim_score(&self, candidate: &Record) -> u64 {
        info!("calculating similarity score");
        let pairs = self
            .into_iter()
            .zip(candidate.into_iter())
            .collect::<Vec<(_, _)>>();
        info!(length = %pairs.len(), " pairs being used");
        let mut interner = INTERNER.write().expect("RwLock is live");
        let score = pairs
            .iter()
            .filter(|pair| pair.0 == pair.1 || pair.1 == interner.get_or_intern_static("*"))
            .fold(0_u64, |acc, _pair| acc + 1);
        info!(%score, "score calculated");
        score
    }

    #[instrument]
    pub fn first(&self) -> Option<DefaultSymbol> {
        match self.tokens.len() {
            0 => None,
            _ => Some(self.tokens[0].clone()),
        }
    }

    #[instrument]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    #[instrument]
    pub fn resolve(&self, sym: DefaultSymbol) -> Option<String> {
        match self.interner.read().unwrap().resolve(sym) {
            Some(s) => Some(s.to_owned()),
            None => None,
        }
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
        if self.index >= self.record.tokens.len() {
            return None;
        }
        let val = self.record.tokens[self.index];
        self.index += 1;
        Some(
            self.record
                .resolve(val)
                .expect("records can resolve their own tokens"),
        )
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
    type Item = DefaultSymbol;
    fn next(&mut self) -> Option<DefaultSymbol> {
        if self.index >= self.record.tokens.len() {
            return None;
        }
        let val = self.record.tokens[self.index];
        self.index += 1;
        Some(val)
    }
}
impl<'a> IntoIterator for &'a Record {
    type Item = DefaultSymbol;
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
        write!(
            f,
            "{}",
            INTERNER
                .read()
                .unwrap()
                .resolve(self.raw_message)
                .expect("interned strings must stay resolvable")
        )
    }
}
#[cfg(test)]
mod should {
    use crate::Record;
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
        let val = rec.first();
        assert_eq!(rec.resolve(val.unwrap()).unwrap(), "Message");
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
        let rec = Record::new(input.clone());
        let tokens = (&rec).into_iter().collect::<Vec<_>>();
        assert_that(&tokens).has_length(7);
    }
}
