// src/query/ast.rs

// Keep existing LogStreamSelector, LabelMatcher, MatcherOp, LineFilterOp, ParseError (and its From impl)
// Ensure the From impl for ParseError uses crate::query::parser::Rule if it's not already.

/// Represents a complete LogQL query.
#[derive(Debug, PartialEq, Clone)]
pub enum Query {
    LogPipeline(LogPipeline),
    // FIXME: Add variants for Metric Queries (e.g., sum(rate(...))) when their structure is defined
    // MetricQuery(MetricQuery),
}

/// Represents a pipeline of operations on log streams.
/// Example: {selector} |= "foo" | json | line_format "{{.field}}"
#[derive(Debug, PartialEq, Clone)]
pub struct LogPipeline {
    pub selector: LogStreamSelector,
    pub stages: Vec<PipelineStage>,
}

/// Represents a single stage in a log processing pipeline.
#[derive(Debug, PartialEq, Clone)]
pub enum PipelineStage {
    LineFilter(LineFilterExpression), // |=, !=, |~, !~ "string"
    ParserExpression(ParserExpression), // | json, | logfmt, | regexp <expr>, | pattern <expr>
    LabelFilterExpression(LabelFilterExpression), // | label_name = "value", | label_name > number (after initial parsing)
    LineFormatExpression(LineFormatExpression), // | line_format "{{.field}}"
    LabelFormatExpression(LabelFormatExpression), // | label_format foo="{{.bar}}"
    UnwrapExpression(UnwrapExpression), // | unwrap <identifier> or | unwrap duration(<identifier>)
    DistinctFilter(DistinctFilter), // | distinct <label1>, <label2>
    // FIXME: Add more stages like DecolorizeExpression, OffsetExpression, etc.
    NotYetImplementedStage(String), // Placeholder for stages not yet fully defined
}

/// Represents a log stream selector.
/// Example: `{label1="value1", label2="value2"}`
#[derive(Debug, PartialEq, Clone)]
pub struct LogStreamSelector {
    pub labels: Vec<LabelMatcher>,
}

/// Represents a label matcher within a stream selector.
/// Example: `label="value"` or `label!="value"` or `label=~"regex"` or `label!~"regex"`
#[derive(Debug, PartialEq, Clone)]
pub struct LabelMatcher {
    pub label: String,
    pub op: MatcherOp,
    pub value: String,
}

/// Represents the matching operation for labels.
#[derive(Debug, PartialEq, Clone)]
pub enum MatcherOp {
    Equal,        // =
    NotEqual,     // !=
    RegexMatch,   // =~
    RegexNoMatch, // !~
}

/// Represents a line filter expression.
#[derive(Debug, PartialEq, Clone)]
pub struct LineFilterExpression {
    pub op: LineFilterOp,
    pub value: String,
}

/// Represents the matching operation for line filters.
#[derive(Debug, PartialEq, Clone)]
pub enum LineFilterOp {
    Contains,        // |=
    NotContains,     // !=
    RegexMatch,      // |~
    RegexNoMatch,    // !~
}

/// Represents a parser expression (e.g., | json, | logfmt <params>).
#[derive(Debug, PartialEq, Clone)]
pub struct ParserExpression {
    pub kind: ParserKind,
    pub params: Option<String>, // For regex, pattern, etc.
}

#[derive(Debug, PartialEq, Clone)]
pub enum ParserKind {
    Json,
    Logfmt,
    Regexp,
    Pattern,
    Unpack,
}

/// Represents a label filter expression (e.g., | code >= 500).
#[derive(Debug, PartialEq, Clone)]
pub struct LabelFilterExpression {
    pub left_label: String,
    pub op: LabelFilterOp,
    pub right_value: String,
}

#[derive(Debug, PartialEq, Clone)]
pub enum LabelFilterOp {
    Equal,        // ==
    NotEqual,     // !=
    GreaterThan,  // >
    LessThan,     // <
    GreaterEqual, // >=
    LessEqual,    // <=
    RegexMatch,   // =~
    RegexNoMatch, // !~
}

/// Represents a line formatting expression (e.g., | line_format "{{ .message }}").
#[derive(Debug, PartialEq, Clone)]
pub struct LineFormatExpression {
    pub template: String,
}

/// Represents a label formatting expression (e.g., | label_format new_label="{{ .extracted_field }}").
#[derive(Debug, PartialEq, Clone)]
pub struct LabelFormatExpression {
    pub formats: Vec<LabelAssignment>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct LabelAssignment {
    pub label_name: String,
    pub template_or_value: String,
}

/// Represents an unwrap expression (e.g., | unwrap latency_ms).
#[derive(Debug, PartialEq, Clone)]
pub struct UnwrapExpression {
    pub identifier: String,
    pub as_duration: bool,
}

/// Represents a distinct filter (e.g., | distinct error, remote_ip).
#[derive(Debug, PartialEq, Clone)]
pub struct DistinctFilter {
    pub labels: Vec<String>,
}

/// Represents a parse error.
#[derive(Debug, PartialEq, Clone)]
pub enum ParseError {
    PestError(String),
    AstConstructionError(String),
    NotYetImplemented(String),
}

impl From<pest::error::Error<crate::query::parser::Rule>> for ParseError {
    fn from(err: pest::error::Error<crate::query::parser::Rule>) -> Self {
        ParseError::PestError(err.to_string())
    }
}
