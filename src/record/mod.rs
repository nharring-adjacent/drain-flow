pub mod tokens;
extern crate derive_more;

use itertools::Itertools;
use rksuid::rksuid;

#[derive(Clone, Debug)]
pub struct Record<'a> {
    pub raw_message: &'a str,
    tokens: Vec<(usize, usize)>,
    pos: usize,
    pub uid: rksuid::Ksuid,
}
impl<'a> Record<'a> {
    pub fn new(line: &'a str) -> Self {
        Self {
            raw_message: line.clone(),
            tokens: line
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
                .collect::<Vec<(usize, usize)>>(),
            pos: 0,
            uid: rksuid::new(None, None),
        }
    }

    pub fn calc_sim_score(self, candidate: &'a Record) -> u64 {
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

    pub fn first(&'a self) -> Option<&'a str> {
        let (start, end) = self.tokens[0];
        self.raw_message.get(start..end)
    }

    pub fn len(&'a self) -> usize {
        self.tokens.len()
    }
}
impl<'a> Iterator for Record<'a> {
    type Item = &'a str;
    fn next(self: &mut Record<'a>) -> Option<Self::Item> {
        let pos = self.pos;
        if pos >= self.tokens.len() {
            return None;
        }

        let (start, end) = self.tokens[pos];
        self.pos += 1;
        self.raw_message.get(start..end)
    }
}
#[cfg(test)]
mod should {
    use crate::Record;
    use spectral::prelude::*;
    #[test]
    fn test_record_new() {
        let input = "Message send failed to remote host: foo.bar.com";
        let rec = Record::new(input);
        assert_eq!(input, rec.raw_message);
    }
    #[test]
    fn test_record_calc_sim_score() {
        let input = "Message send failed to remote host: foo.bar.com";
        let input2 = "Message send succeeded with flying colors";
        let rec = Record::new(input);
        let other = Record::new(input2);
        let score = rec.calc_sim_score(&other);
        assert_eq!(score, 2);
    }
    #[test]
    fn test_record_first() {
        let input = "Message send failed to remote host: foo.bar.com";
        let rec = Record::new(input);
        assert_eq!(rec.first(), Some("Message"));
    }
    #[test]
    fn test_record_len() {
        let input = "Message send failed to remote host: foo.bar.com";
        let rec = Record::new(input);
        assert_eq!(rec.len(), 7);
    }

    #[test]
    fn test_into_iter() {
        let input = "Message send failed to remote host: foo.bar.com";
        let rec = Record::new(input.clone());
        let tokens = rec.into_iter().collect::<Vec<&str>>();
        let words = &input
            .split(|c: char| c.is_whitespace())
            .collect::<Vec<&str>>();
        assert_that(&tokens.iter()).contains_all_of(&words.iter());
    }
}
