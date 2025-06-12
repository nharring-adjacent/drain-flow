// src/query/parser.rs

use pest::Parser;
use crate::query::ast::{self, LabelMatcher, LineFilterExpression, LogStreamSelector, MatcherOp, LineFilterOp, Query as AstQuery, ParseError, LogPipeline, PipelineStage};

#[derive(pest_derive::Parser)]
#[grammar = "query/logql.pest"]
pub struct LogQlParser;

pub fn parse_logql_query(query_str: &str) -> Result<AstQuery, ParseError> {
    match LogQlParser::parse(Rule::query, query_str) {
        Ok(mut pairs) => {
            let query_pair = pairs.next().ok_or_else(|| ParseError::AstConstructionError("Expected a query rule, but found none.".to_string()))?;
            if query_pair.as_rule() != Rule::query {
                return Err(ParseError::AstConstructionError(format!("Expected a query rule, but found {:?}", query_pair.as_rule())));
            }

            let mut inner_rules = query_pair.into_inner();

            let stream_selector_pair = inner_rules.next()
                .ok_or_else(|| ParseError::AstConstructionError("Expected log_stream_selector in query".to_string()))?;
            if stream_selector_pair.as_rule() != Rule::log_stream_selector {
                 return Err(ParseError::AstConstructionError(format!("Expected log_stream_selector, found {:?}", stream_selector_pair.as_rule())));
            }
            let stream_selector = parse_log_stream_selector(stream_selector_pair)?;

            let mut stages = Vec::new();
            for pair in inner_rules {
                match pair.as_rule() {
                    Rule::line_filter_expr => {
                        // For now, treat as a stub. Full parsing would call parse_line_filter.
                        // stages.push(PipelineStage::LineFilter(parse_line_filter(pair)?));
                        stages.push(PipelineStage::NotYetImplementedStage(format!("LineFilter: {}", pair.as_str())));
                    }
                    // FIXME: Add cases for other pipeline stage rules from logql.pest
                    // when they are added to the grammar (e.g., Rule::parser_expr, Rule::label_filter_expr etc.)
                    // Example:
                    // Rule::parser_expr => {
                    //     stages.push(PipelineStage::NotYetImplementedStage(format!("ParserExpression: {}", pair.as_str())));
                    // }
                    Rule::EOI => break,
                    _ => return Err(ParseError::AstConstructionError(format!("Unexpected rule in query pipeline: {:?}", pair.as_rule()))),
                }
            }

            Ok(AstQuery::LogPipeline(LogPipeline {
                selector: stream_selector,
                stages,
            }))
        }
        Err(e) => Err(ParseError::PestError(e.to_string())),
    }
}

fn parse_log_stream_selector(pair: pest::iterators::Pair<Rule>) -> Result<LogStreamSelector, ParseError> {
    if pair.as_rule() != Rule::log_stream_selector {
        return Err(ParseError::AstConstructionError(format!("Expected log_stream_selector, found {:?}", pair.as_rule())));
    }

    let mut labels = Vec::new();
    for inner_pair in pair.into_inner() {
        match inner_pair.as_rule() {
            Rule::label_matcher => {
                labels.push(parse_label_matcher(inner_pair)?);
            }
            _ => return Err(ParseError::AstConstructionError(format!("Unexpected rule in log_stream_selector: {:?}", inner_pair.as_rule()))),
        }
    }
    Ok(LogStreamSelector { labels })
}

fn parse_label_matcher(pair: pest::iterators::Pair<Rule>) -> Result<LabelMatcher, ParseError> {
    if pair.as_rule() != Rule::label_matcher {
        return Err(ParseError::AstConstructionError(format!("Expected label_matcher, found {:?}", pair.as_rule())));
    }

    let mut inner_rules = pair.into_inner();
    let label_name_pair = inner_rules.next().ok_or_else(|| ParseError::AstConstructionError("Expected label_name in label_matcher".to_string()))?;
    let matcher_op_pair = inner_rules.next().ok_or_else(|| ParseError::AstConstructionError("Expected matcher_op in label_matcher".to_string()))?;
    let quoted_string_pair = inner_rules.next().ok_or_else(|| ParseError::AstConstructionError("Expected quoted_string in label_matcher".to_string()))?;

    let label = label_name_pair.as_str().to_string();

    let op = match matcher_op_pair.as_str() {
        "=" => MatcherOp::Equal,
        "!=" => MatcherOp::NotEqual,
        "=~" => MatcherOp::RegexMatch,
        "!~" => MatcherOp::RegexNoMatch,
        _ => return Err(ParseError::AstConstructionError(format!("Unknown matcher_op: {}", matcher_op_pair.as_str()))),
    };

    let value_str_raw = quoted_string_pair.as_str();
    let value = value_str_raw.trim_start_matches(''').trim_end_matches(''').trim_start_matches('"').trim_end_matches('"').to_string();
    // FIXME: Add proper unescaping of string literals

    Ok(LabelMatcher { label, op, value })
}

#[allow(dead_code)]
fn parse_line_filter(pair: pest::iterators::Pair<Rule>) -> Result<LineFilterExpression, ParseError> {
    if pair.as_rule() != Rule::line_filter_expr {
        return Err(ParseError::AstConstructionError(format!("Expected line_filter_expr, found {:?}", pair.as_rule())));
    }

    let mut inner_rules = pair.into_inner();
    let line_filter_op_pair = inner_rules.next().ok_or_else(|| ParseError::AstConstructionError("Expected line_filter_op in line_filter_expr".to_string()))?;
    let quoted_string_pair = inner_rules.next().ok_or_else(|| ParseError::AstConstructionError("Expected quoted_string in line_filter_expr".to_string()))?;

    let op = match line_filter_op_pair.as_str() {
        "|=" => LineFilterOp::Contains,
        "!=" => LineFilterOp::NotContains,
        "|~" => LineFilterOp::RegexMatch,
        "!~" => LineFilterOp::RegexNoMatch,
        _ => return Err(ParseError::AstConstructionError(format!("Unknown line_filter_op: {}", line_filter_op_pair.as_str()))),
    };

    let value_str_raw = quoted_string_pair.as_str();
    let value = value_str_raw.trim_start_matches(''').trim_end_matches(''').trim_start_matches('"').trim_end_matches('"').to_string();
    // FIXME: Add proper unescaping for line filter values

    Ok(LineFilterExpression { op, value })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::ast::{LabelMatcher, LogStreamSelector, MatcherOp, Query as AstQuery, LogPipeline, PipelineStage, ParseError}; // Ensure ParseError is imported

    #[test]
    fn test_parse_simple_stream_selector_pipeline() {
        let query_str = "{app="foo"}";
        let expected_ast = AstQuery::LogPipeline(LogPipeline {
            selector: LogStreamSelector {
                labels: vec![LabelMatcher {
                    label: "app".to_string(),
                    op: MatcherOp::Equal,
                    value: "foo".to_string(),
                }],
            },
            stages: vec![],
        });
        match parse_logql_query(query_str) {
            Ok(ast) => assert_eq!(ast, expected_ast),
            Err(e) => panic!("Parsing failed: {:?}", e),
        }
    }

    #[test]
    fn test_parse_stream_selector_with_multiple_labels() {
        let query_str = "{app="foo",env!="prod",zone=~"us-.+",method!~"get|post"}";
        let expected_ast = AstQuery::LogPipeline(LogPipeline {
            selector: LogStreamSelector {
                labels: vec![
                    LabelMatcher {
                        label: "app".to_string(),
                        op: MatcherOp::Equal,
                        value: "foo".to_string(),
                    },
                    LabelMatcher {
                        label: "env".to_string(),
                        op: MatcherOp::NotEqual,
                        value: "prod".to_string(),
                    },
                    LabelMatcher {
                        label: "zone".to_string(),
                        op: MatcherOp::RegexMatch,
                        value: "us-.+".to_string(),
                    },
                    LabelMatcher {
                        label: "method".to_string(),
                        op: MatcherOp::RegexNoMatch,
                        value: "get|post".to_string(),
                    },
                ],
            },
            stages: vec![],
        });
         match parse_logql_query(query_str) {
            Ok(ast) => assert_eq!(ast, expected_ast),
            Err(e) => panic!("Parsing failed: {:?}", e),
        }
    }

    #[test]
    fn test_parse_empty_stream_selector() {
        let query_str = "{}";
        let expected_ast = AstQuery::LogPipeline(LogPipeline {
            selector: LogStreamSelector { labels: vec![] },
            stages: vec![],
        });
        match parse_logql_query(query_str) {
            Ok(ast) => assert_eq!(ast, expected_ast),
            Err(e) => panic!("Parsing failed: {:?}", e),
        }
    }

    #[test]
    fn test_parse_stream_selector_with_whitespace() {
        let query_str = "{ app = "foo" , env != "prod" }";
        let expected_ast = AstQuery::LogPipeline(LogPipeline {
            selector: LogStreamSelector {
                labels: vec![
                    LabelMatcher {
                        label: "app".to_string(),
                        op: MatcherOp::Equal,
                        value: "foo".to_string(),
                    },
                    LabelMatcher {
                        label: "env".to_string(),
                        op: MatcherOp::NotEqual,
                        value: "prod".to_string(),
                    },
                ],
            },
            stages: vec![],
        });
        match parse_logql_query(query_str) {
            Ok(ast) => assert_eq!(ast, expected_ast),
            Err(e) => panic!("Parsing failed for whitespace test: {:?}", e),
        }
    }

    #[test]
    fn test_parse_stream_selector_label_with_hyphen_and_underscore() {
        let query_str = "{app_name="my-app", job_id!~"id-\\d+"}"; // Escaped \d for Rust string
        let expected_ast = AstQuery::LogPipeline(LogPipeline {
            selector: LogStreamSelector {
                labels: vec![
                    LabelMatcher {
                        label: "app_name".to_string(),
                        op: MatcherOp::Equal,
                        value: "my-app".to_string(),
                    },
                    LabelMatcher {
                        label: "job_id".to_string(),
                        op: MatcherOp::RegexNoMatch,
                        value: "id-\d+".to_string(), // Compare with unescaped version
                    },
                ],
            },
            stages: vec![],
        });
        match parse_logql_query(query_str) {
            Ok(ast) => assert_eq!(ast, expected_ast),
            Err(e) => panic!("Parsing failed for label with hyphen/underscore: {:?}", e),
        }
    }


    #[test]
    fn test_parse_invalid_syntax_missing_curly_brace_start() {
        let query_str = "app="foo"}";
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for missing start brace");
    }

    #[test]
    fn test_parse_invalid_syntax_missing_curly_brace_end() {
        let query_str = "{app="foo"";
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for missing end brace");
    }

    #[test]
    fn test_parse_invalid_label_matcher_op() {
        let query_str = "{app % "foo"}"; // Invalid operator
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for invalid operator");
    }

    #[test]
    fn test_parse_invalid_label_value_no_quotes() {
        let query_str = "{app=foo}";
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for unquoted label value");
    }

    #[test]
    fn test_parse_invalid_syntax_extra_comma_selector() {
        let query_str = "{app="foo",}";
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for trailing comma in selector");
    }

    #[test]
    fn test_parse_invalid_syntax_leading_comma_selector() {
        let query_str = "{,app="foo"}";
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for leading comma in selector");
    }


    #[test]
    fn test_parse_stream_selector_single_quotes() {
        let query_str = "{app='foo'}";
        let expected_ast = AstQuery::LogPipeline(LogPipeline {
            selector: LogStreamSelector {
                labels: vec![LabelMatcher {
                    label: "app".to_string(),
                    op: MatcherOp::Equal,
                    value: "foo".to_string(),
                }],
            },
            stages: vec![],
        });
        match parse_logql_query(query_str) {
            Ok(ast) => assert_eq!(ast, expected_ast),
            Err(e) => panic!("Parsing failed: {:?}", e),
        }
    }

    #[test]
    fn test_parse_stream_selector_with_line_filter_contains_stub() {
        let query_str = "{app="foo"} |= "error"";
        match parse_logql_query(query_str) {
            Ok(AstQuery::LogPipeline(pipeline)) => {
                assert_eq!(pipeline.selector.labels.len(), 1);
                assert_eq!(pipeline.selector.labels[0].value, "foo");
                assert_eq!(pipeline.stages.len(), 1);
                match &pipeline.stages[0] {
                    PipelineStage::NotYetImplementedStage(s) => {
                        assert!(s.contains("LineFilter"));
                        assert!(s.contains("|="));
                        assert!(s.contains("error"));
                    }
                    _ => panic!("Expected NotYetImplementedStage for line filter |= "),
                }
            }
            Ok(_) => panic!("Expected LogPipeline variant"),
            Err(e) => panic!("Parsing failed for line filter |= stub: {:?}", e),
        }
    }

    #[test]
    fn test_parse_stream_selector_with_line_filter_not_contains_stub() {
        let query_str = "{app!~"bar"} != "info"";
        match parse_logql_query(query_str) {
            Ok(AstQuery::LogPipeline(pipeline)) => {
                assert_eq!(pipeline.selector.labels.len(), 1);
                assert_eq!(pipeline.selector.labels[0].op, MatcherOp::RegexNoMatch);
                assert_eq!(pipeline.stages.len(), 1);
                match &pipeline.stages[0] {
                    PipelineStage::NotYetImplementedStage(s) => {
                        assert!(s.contains("LineFilter"));
                        assert!(s.contains("!="));
                        assert!(s.contains("info"));
                    }
                    _ => panic!("Expected NotYetImplementedStage for line filter != "),
                }
            }
            Ok(_) => panic!("Expected LogPipeline variant"),
            Err(e) => panic!("Parsing failed for line filter != stub: {:?}", e),
        }
    }


    #[test]
    fn test_parse_stream_selector_with_line_filter_regex_match_stub() {
        let query_str = "{app="bar"} |~ "info.*"";
         match parse_logql_query(query_str) {
            Ok(AstQuery::LogPipeline(pipeline)) => {
                assert_eq!(pipeline.stages.len(), 1);
                match &pipeline.stages[0] {
                    PipelineStage::NotYetImplementedStage(s) => {
                        assert!(s.contains("LineFilter"));
                        assert!(s.contains("|~"));
                        assert!(s.contains("info.*"));
                    }
                    _ => panic!("Expected NotYetImplementedStage for line filter |~ "),
                }
            }
            Ok(_) => panic!("Expected LogPipeline variant"),
            Err(e) => panic!("Parsing failed for line filter |~ stub: {:?}", e),
        }
    }

    #[test]
    fn test_parse_stream_selector_with_line_filter_regex_no_match_stub() {
        let query_str = "{job='writer'} !~ ".*read.*""; // Single quotes for filter value
         match parse_logql_query(query_str) {
            Ok(AstQuery::LogPipeline(pipeline)) => {
                assert_eq!(pipeline.stages.len(), 1);
                match &pipeline.stages[0] {
                    PipelineStage::NotYetImplementedStage(s) => {
                        assert!(s.contains("LineFilter"));
                        assert!(s.contains("!~"));
                        assert!(s.contains(".*read.*"));
                    }
                    _ => panic!("Expected NotYetImplementedStage for line filter !~ "),
                }
            }
            Ok(_) => panic!("Expected LogPipeline variant"),
            Err(e) => panic!("Parsing failed for line filter !~ stub: {:?}", e),
        }
    }


    #[test]
    fn test_parse_stream_selector_with_multiple_line_filters_stub() {
        let query_str = "{app="foo"} |= "error" |~ "pattern.*" != "false_positive" !~ "noisy"";
        match parse_logql_query(query_str) {
            Ok(AstQuery::LogPipeline(pipeline)) => {
                assert_eq!(pipeline.selector.labels.len(), 1);
                assert_eq!(pipeline.stages.len(), 4);

                let stage_texts: Vec<String> = pipeline.stages.iter().map(|s| {
                    if let PipelineStage::NotYetImplementedStage(text) = s {
                        text.clone()
                    } else {
                        panic!("Expected only NotYetImplementedStage");
                    }
                }).collect();

                assert!(stage_texts[0].contains("|=") && stage_texts[0].contains("error"));
                assert!(stage_texts[1].contains("|~") && stage_texts[1].contains("pattern.*"));
                assert!(stage_texts[2].contains("!=") && stage_texts[2].contains("false_positive"));
                assert!(stage_texts[3].contains("!~") && stage_texts[3].contains("noisy"));
            }
            Ok(_) => panic!("Expected LogPipeline variant"),
            Err(e) => panic!("Parsing failed for multiple line filter stubs: {:?}", e),
        }
    }

    #[test]
    fn test_parse_invalid_line_filter_op() {
        // This test relies on the grammar being strict about filter operators.
        // If the grammar `line_filter_op` is simply `_ ~ _`, this might pass pest
        // and fail in AST construction or later validation.
        // Current `logql.pest` is specific: `line_filter_op = { "|=" | "!=" | "|~" | "!~" }`
        let query_str = "{app="foo"} |x "error""; // Invalid filter operator
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for invalid line filter operator");
    }

    #[test]
    fn test_parse_invalid_line_filter_no_quotes() {
        let query_str = "{app="foo"} |= error";
        assert!(matches!(parse_logql_query(query_str), Err(ParseError::PestError(_))), "Expected PestError for unquoted line filter value");
    }
}
