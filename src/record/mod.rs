pub mod tokens;
extern crate derive_more;
use itertools::Itertools;
use lazy_static::lazy_static;
use rksuid::rksuid;
use std::fmt;
use std::sync::Mutex;
use string_interner::{DefaultSymbol, StringInterner};

lazy_static! {
    static ref INTERNER: Mutex<StringInterner> = Mutex::new(StringInterner::default());
}
#[derive(Clone, Debug)]
pub struct Record {
    interner: &'static Mutex<StringInterner>,
    pub raw_message: DefaultSymbol,
    tokens: Vec<DefaultSymbol>,
    pos: usize,
    pub uid: rksuid::Ksuid,
}
impl Record {
    pub fn new(line: String) -> Self {
        Self {
            interner: &INTERNER,
            raw_message: INTERNER.lock().unwrap().get_or_intern(line.clone()),
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
                .map(|t| {
                    INTERNER
                        .lock()
                        .unwrap()
                        .get_or_intern(line.as_str().get(t.0..t.1).unwrap().to_owned())
                })
                .collect::<Vec<DefaultSymbol>>(),
            pos: 0,
            uid: rksuid::new(None, None),
        }
    }

    pub fn calc_sim_score(self, candidate: &Record) -> u64 {
        let pairs = self
            .into_iter()
            .zip(candidate.clone().into_iter())
            .collect::<Vec<(_, _)>>();

        let score = pairs
            .iter()
            .filter(|pair| pair.0 == pair.1 || pair.1 == "*")
            .fold(0_u64, |acc, _pair| acc + 1);
        score
    }

    pub fn first(&self) -> DefaultSymbol {
        self.tokens[0].clone()
    }

    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn resolve(&self, sym: DefaultSymbol) -> Option<String> {
        match self.interner.lock().unwrap().resolve(sym) {
            Some(s) => Some(s.to_owned()),
            None => None,
        }
    }
}

impl Iterator for Record {
    type Item = String;
    fn next(self: &mut Record) -> Option<Self::Item> {
        let pos = self.pos;
        if pos >= self.tokens.len() {
            return None;
        }

        let sym = self.tokens[pos];
        self.pos += 1;
        self.resolve(sym)
    }
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            INTERNER
                .lock()
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
    #[test]
    fn test_record_new() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input.clone());
        assert_eq!(input, rec.to_string());
    }
    #[test]
    fn test_record_calc_sim_score() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let input2 = "Message send succeeded with flying colors".to_string();
        let rec = Record::new(input);
        let other = Record::new(input2);
        let score = rec.calc_sim_score(&other);
        assert_eq!(score, 2);
    }
    #[test]
    fn test_record_first() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        let val = rec.first();
        assert_eq!(rec.resolve(val).unwrap(), "Message");
    }
    #[test]
    fn test_record_len() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input);
        assert_eq!(rec.len(), 7);
    }

    #[test]
    fn test_into_iter() {
        let input = "Message send failed to remote host: foo.bar.com".to_string();
        let rec = Record::new(input.clone());
        let tokens = rec.into_iter().collect::<Vec<String>>();
        let words = &input
            .split(|c: char| c.is_whitespace())
            .map(|s| s.to_owned())
            .collect::<Vec<String>>();
        assert_that(&tokens.iter()).contains_all_of(&words.iter());
    }
}
