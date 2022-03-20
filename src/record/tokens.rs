use float_eq::float_eq;

#[derive(Debug, Clone)]
pub enum Token {
    /// Token that matches any other token
    Wildcard,
    /// Token that matches any value of the inner type
    TypedMatch(TypedToken),
    /// Token containing a typed, non-wildcard value
    Value(TypedToken),
}

#[derive(PartialEq, Debug, Clone)]
pub enum TypedToken {
    /// Token containing a string with at least 1 non-digit
    String(String),
    /// Token containing a whole number only
    Int(i64),
    /// Token containing a float
    Float(f64),
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        return match self {
            Token::Wildcard => true,
            Token::TypedMatch(tm) => match other {
                Token::Wildcard => true,
                Token::TypedMatch(otm) => tm == otm,
                Token::Value(other_val) => match tm {
                    TypedToken::String(_) => {
                        if let TypedToken::String(_) = other_val {
                            return true;
                        }
                        false
                    },
                    TypedToken::Int(_) => {
                        if let TypedToken::Int(_) = other_val {
                            return true;
                        }
                        false
                    },
                    TypedToken::Float(_) => {
                        if let TypedToken::Float(_) = other_val {
                            return true;
                        }
                        true
                    },
                },
            },
            Token::Value(val) => match other {
                Token::Wildcard => true,
                Token::TypedMatch(tm) => {
                    match val {
                        TypedToken::String(_) => {
                            if let TypedToken::String(_) = tm {
                                return true;
                            }
                            false 
                        },
                        TypedToken::Int(_) => {
                            if let TypedToken::Int(_) = tm {
                                return true;
                            }
                            false
                        },
                        TypedToken::Float(_) => {
                            if let TypedToken::Float(_) = tm {
                                return true
                            }
                            false
                        },
                    }
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
        };
    }
}

impl Eq for Token {}

#[cfg(test)]
mod should {
    use crate::record::tokens::{Token, TypedToken};
    use spectral::prelude::*;
    use proptest::prelude::*;

    #[test]
    fn test_wildcard_lhs() {
        let lhs = Token::Wildcard;
        let rhs = Token::Value(TypedToken::String("foo".to_string()));
        assert_that(&lhs).is_equal_to(rhs.clone());
        assert_that(&rhs).is_equal_to(lhs);
    }

    proptest! {
        #[test]
        fn test_wildcard_matches_any_string(s in "\\PC*") {
            let wildcard = Token::Wildcard;
            let val = Token::Value(TypedToken::String(s));
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
        fn test_typedmatch_string_matches_any_string(s in "\\PC*") {
            let tm = Token::TypedMatch(TypedToken::String(String::from("")));
            let val = Token::Value(TypedToken::String(s));
            assert_that(&tm).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(tm);
        }

        #[test]
        fn test_typedmatch_int_matches_any_int(s in i64::MIN..i64::MAX) {
            let tm = Token::TypedMatch(TypedToken::Int(0));
            let val = Token::Value(TypedToken::Int(s));
            assert_that(&tm).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(tm);
        }

        #[test]
        fn test_typedmatch_float_matches_any_positive_float(s in 0f64..f64::MAX) {
            let tm = Token::TypedMatch(TypedToken::Float(0.0));
            let val = Token::Value(TypedToken::Float(s));
            assert_that(&tm).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(tm);
        }

        #[test]
        fn test_typedmatch_float_matches_any_negative_float(s in f64::MIN..0f64) {
            let tm = Token::TypedMatch(TypedToken::Float(0.0));
            let val = Token::Value(TypedToken::Float(s));
            assert_that(&tm).is_equal_to(val.clone());
            assert_that(&val).is_equal_to(tm);
        }

        #[test]
        fn test_value_string_matches_same_string(s in "\\PC*") {
            let val1 = Token::Value(TypedToken::String(s.clone()));
            let val2 = Token::Value(TypedToken::String(s));
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
