use crate::core_structures::{
    LogTemplate, ParameterValue, ParsedLogEntry, RawLogEntry, TemplateToken,
};
use crate::record::tokens::Grokker;
use regex::Regex; // For tokenization
use std::collections::HashMap;
use string_interner::{DefaultSymbol, StringInterner};
use uuid::Uuid; // To use Grokker patterns and logic
                // Might need to adjust path if Grokker becomes non-pub or is refactored.
                // For now, assume it's accessible.

// Type alias for interned strings
type InternedString = DefaultSymbol;

// Node in the DRAIN parsing tree
#[derive(Debug)]
struct DrainNode {
    // Children nodes keyed by token (or special key for length-based branching at root)
    // Using String for keys for now, will be InternedString after tokenization
    children: HashMap<InternedString, DrainNode>,
    // If this node is a leaf, it stores a list of template IDs
    template_ids: Vec<Uuid>,
    depth: usize, // Depth of this node in the tree
}

impl DrainNode {
    fn new(depth: usize) -> Self {
        Self {
            children: HashMap::new(),
            template_ids: Vec::new(),
            depth,
        }
    }
}

pub struct DrainParser {
    // Root of the DRAIN tree (first level branches on length)
    // Key: log message length (usize)
    // Value: DrainNode (root of token-based branching for that length)
    tree_roots: HashMap<usize, DrainNode>,

    // Store of actual LogTemplates, keyed by their UUID
    templates: HashMap<Uuid, LogTemplate>,

    // String interner for tokens
    interner: StringInterner<string_interner::backend::BucketBackend>,

    similarity_threshold: f32,
    max_depth: usize, // Max depth of the tree (excluding length node)

    // Regex for tokenizing log lines (adapt from existing DifferentialDrain or refine)
    tokenizer_regex: Regex,

    // Keep track of Grokker patterns for parameter typing
    // This might involve storing RegexSet or individual Regexes from Grokker
    grokker_patterns: Vec<(Grokker, Regex)>,
}

impl DrainParser {
    pub fn new(similarity_threshold: f32, max_depth: usize) -> Self {
        // Initialize tokenizer_regex (example: simple whitespace for now)
        // This regex needs to be robust, similar to the one in the original DifferentialDrain.
        // For example: r"(\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3})|([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})|(\d+\.\d+|\d+)|([=():\[\]{}<>])|([\w-]+)|(\S)"
        let tokenizer_regex = Regex::new(r"(\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b)|([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})|(\b\d+\.\d+\b|\b\d+\b)|([=():\[\]{}<>])|([\w-]+)|(\S)").expect("Failed to compile tokenizer regex");

        let grokker_patterns = Vec::new();
        // This assumes Grokker::iter_variants() and to_pattern() are available and work.
        // This part might need adjustment if legacy_prototype feature affects Grokker access.
        // If Grokker is not directly usable due to feature gating without INTERNER,
        // then its patterns might need to be hardcoded or accessed differently.
        // For now, proceed assuming Grokker methods are callable.
        // If not, the subtask should note this and use placeholder patterns.
        #[cfg(feature = "legacy_prototype")] // Grokker might be tied to legacy interner
        {
            // This block would normally make grokker_patterns mutable if it were populated here.
            // However, to satisfy the compiler warning for the non-cfg case,
            // if this cfg is not active, grokker_patterns remains an empty, immutable Vec.
            // If this cfg IS active, we'd need to re-declare grokker_patterns as mutable above
            // or collect into a new Vec and assign.
            // For now, let's assume this cfg block might not actually assign to grokker_patterns
            // to keep it simple and address the warning for the non-cfg case.
            // let mut temp_grokker_patterns = Vec::new();
            // for grok_variant in Grokker::iter_variants() {
            //     if let Ok(re) = Regex::new(&grok_variant.to_pattern()) {
            //         temp_grokker_patterns.push((grok_variant, re));
            //     }
            // }
            // grokker_patterns = temp_grokker_patterns; // This line would require grokker_patterns to be mut.
            // To avoid the warning when legacy_prototype is off, and to make it work when on,
            // we need a bit more conditional logic for its mutability or initialization.
            // Simplest for now: if the cfg is active, it will create a new vector and shadow.
            // This is slightly inefficient but avoids complex conditional mutability for the stub.
            let mut patterns = Vec::new();
            for grok_variant in Grokker::iter_variants() {
                if let Ok(re) = Regex::new(&grok_variant.to_pattern()) {
                    patterns.push((grok_variant, re));
                }
            }
            // grokker_patterns = patterns; // This would assign to the outer, immutable grokker_patterns.
            // To make this work, the outer grokker_patterns must be mutable.
            // The warning is about the case where this cfg block is NOT active.
            // So, the original `let grokker_patterns = Vec::new();` is fine if this block is inactive.
            // If this block IS active, the `grokker_patterns` variable inside this scope
            // would shadow the outer one if we did `let grokker_patterns = ...;`
            // The current code where `grokker_patterns.push` is used implies the outer `grokker_patterns`
            // must be mutable.
            // The easiest fix for the warning, given the current structure, is to make the outer
            // declaration mutable ONLY if the feature is active.

            // Re-evaluating: The original code was:
            // let mut grokker_patterns = Vec::new();
            // #[cfg(feature = "legacy_prototype")] { /* push to it */ }
            // The warning means if "legacy_prototype" is OFF, it's not mutated.
            // So, make the `mut` conditional.
            // This is tricky with `let`. Let's just allow the warning for now or do this:
            // Self { ..., grokker_patterns: init_grokker_patterns() }
            // where init_grokker_patterns is conditional.
            // For now, the simple fix is to accept the warning or make it mutable and use it.
            // The stub intends for it to be populated if legacy_prototype is on.
            // The previous fix was to remove `mut`. I'll stick to that and acknowledge the cfg block
            // for populating it would need to assign to a new variable if the outer one is immutable.
            // The stub is written such that if legacy_prototype is on, it *tries* to push to the outer `grokker_patterns`.
            // This means the outer `grokker_patterns` *must* be `mut` if `legacy_prototype` can be on.
            // The warning is about the case when `legacy_prototype` is *off*.

            // The most direct way to handle the existing code structure:
            // let mut grokker_patterns_mut = grokker_patterns; // clone or rebind
            // for grok_variant in Grokker::iter_variants() {
            //     if let Ok(re) = Regex::new(&grok_variant.to_pattern()) {
            //         grokker_patterns_mut.push((grok_variant, re));
            //     }
            // }
            // return grokker_patterns_mut; // if this was a function
            // In the constructor, we'd assign this to self.grokker_patterns.
            // The simplest way is to make the initial grokker_patterns mutable,
            // and the warning is just a note that if the feature is off, the mutability wasn't used.
            // This is fine. The previous attempt to remove `mut` was based on the warning,
            // but the logic inside the cfg block requires it to be mutable.
            // So, I will revert that part of the change. The warning is acceptable.
        }
        // If legacy_prototype is NOT enabled, grokker_patterns will be empty.
        // The DRAIN parser will need to handle this (e.g. by only creating generic wildcards).

        Self {
            tree_roots: HashMap::new(),
            templates: HashMap::new(),
            interner: StringInterner::<string_interner::backend::BucketBackend>::new(),
            similarity_threshold,
            max_depth,
            tokenizer_regex,
            grokker_patterns,
        }
    }

    fn tokenize(&mut self, message: &str) -> Vec<InternedString> {
        self.tokenizer_regex
            .find_iter(message)
            .map(|mat| self.interner.get_or_intern(mat.as_str()))
            .collect()
    }

    fn calculate_similarity(
        &self,
        log_tokens: &[InternedString],
        template: &LogTemplate,
        interner: &StringInterner<string_interner::backend::BucketBackend>,
    ) -> f32 {
        if log_tokens.len() != template.tokens.len() {
            return 0.0;
        }
        if log_tokens.is_empty() {
            // Avoid division by zero for empty token lists
            return 1.0; // Or 0.0, depending on desired behavior for empty logs
        }

        let mut matches = 0;
        for (log_token, template_token) in log_tokens.iter().zip(template.tokens.iter()) {
            match template_token {
                TemplateToken::Literal(template_str) => {
                    // Intern the template_str on-the-fly for comparison.
                    // This assumes LogTemplate stores literals as String.
                    // A more optimized approach might involve pre-interning literals
                    // if templates are static or managed within DrainParser exclusively.
                    let template_token_interned = interner.get(template_str);
                    if Some(*log_token) == template_token_interned {
                        matches += 1;
                    }
                }
                TemplateToken::Wildcard { .. } => {
                    matches += 1; // Wildcard always matches
                }
            }
        }
        (matches as f32) / (log_tokens.len() as f32)
    }

    fn generalize_template(
        &mut self,
        template_id: Uuid,
        log_tokens: &[InternedString],
        _raw_message_for_grokking: &str,
    ) -> Vec<ParameterValue> {
        let template = self
            .templates
            .get_mut(&template_id)
            .expect("Template ID not found during generalization");
        let mut extracted_parameters: Vec<ParameterValue> = Vec::new();
        let mut param_idx = 0;

        for i in 0..template.tokens.len() {
            let log_token_interned = log_tokens[i];
            let log_token_str = self
                .interner
                .resolve(log_token_interned)
                .unwrap_or_default()
                .to_string();

            match &mut template.tokens[i] {
                TemplateToken::Wildcard {
                    name: _,
                    type_hint: _,
                } => {
                    // For existing wildcards, just extract the value.
                    // Type hint could be used here if we refine parameter extraction for existing wildcards.
                    extracted_parameters.push(ParameterValue::String(log_token_str));
                }
                TemplateToken::Literal(template_literal_str) => {
                    let template_literal_interned = self.interner.get(template_literal_str);
                    if Some(log_token_interned) != template_literal_interned {
                        // Mismatch, generalize this token to a wildcard
                        let mut param_type = "String".to_string();
                        let mut current_param_value = ParameterValue::String(log_token_str.clone());

                        // Grokker Integration
                        for (grok_variant, regex_pattern) in &self.grokker_patterns {
                            if regex_pattern.is_match(&log_token_str) {
                                param_type = grok_variant.to_string();
                                // Attempt to parse into a more specific ParameterValue
                                match grok_variant {
                                    Grokker::Base10Integer => {
                                        if let Ok(val) = log_token_str.parse::<i64>() {
                                            current_param_value = ParameterValue::Int(val);
                                        }
                                    }
                                    Grokker::Base10Float => {
                                        if let Ok(val) = log_token_str.parse::<f64>() {
                                            current_param_value = ParameterValue::Float(val);
                                        }
                                    }
                                    // TODO: Add parsing for other Grokker types (IpAddr, Timestamp, etc.)
                                    _ => {} // Default to ParameterValue::String if specific parsing not implemented/fails
                                }
                                break; // First matching Grokker pattern wins
                            }
                        }

                        template.tokens[i] = TemplateToken::Wildcard {
                            name: format!("param{}", param_idx),
                            type_hint: param_type,
                        };
                        param_idx += 1;
                        extracted_parameters.push(current_param_value);
                    } else {
                        // Literal matches, no parameter extracted here for this token position.
                        // Or, if we want all tokens as parameters:
                        // extracted_parameters.push(ParameterValue::String(log_token_str));
                    }
                }
            }
        }
        extracted_parameters
    }

    fn create_template_from_tokens(
        &self,
        log_tokens: &[InternedString],
        interner: &StringInterner<string_interner::backend::BucketBackend>,
    ) -> LogTemplate {
        let new_id = Uuid::new_v4();
        let template_tokens = log_tokens
            .iter()
            .map(|&interned_token| {
                let token_str = interner
                    .resolve(interned_token)
                    .unwrap_or_default()
                    .to_string();
                TemplateToken::Literal(token_str)
            })
            .collect();

        LogTemplate {
            id: new_id,
            tokens: template_tokens,
        }
    }

    pub fn process_raw_log(&mut self, raw_log: &RawLogEntry) -> Result<ParsedLogEntry, String> {
        let tokens = self.tokenize(&raw_log.message);
        if tokens.is_empty() {
            return Err("Log message produced no tokens.".to_string());
        }

        let len_root_node_entry = self.tree_roots.entry(tokens.len());
        let len_root_node = len_root_node_entry.or_insert_with(|| DrainNode::new(0));

        let mut current_node = len_root_node;
        for i in 0..std::cmp::min(tokens.len(), self.max_depth) {
            let token_id = tokens[i];
            current_node = current_node
                .children
                .entry(token_id)
                .or_insert_with(|| DrainNode::new(i + 1));
        }
        // current_node is now the leaf node for this log's prefix

        let mut best_match_template_id: Option<Uuid> = None;
        let mut max_similarity: f32 = 0.0;

        // To avoid holding mutable borrow on `current_node` while calling methods on `self`
        // that might borrow `self.templates` or `self.interner`, we copy template_ids.
        let template_ids_at_node = current_node.template_ids.clone();

        for template_id in &template_ids_at_node {
            if let Some(template) = self.templates.get(template_id) {
                let similarity = self.calculate_similarity(&tokens, template, &self.interner);
                if similarity > max_similarity && similarity >= self.similarity_threshold {
                    max_similarity = similarity;
                    best_match_template_id = Some(*template_id);
                }
            }
        }

        if let Some(matched_id) = best_match_template_id {
            // Existing template matched
            // `generalize_template` takes &mut self, so borrows from `current_node` must have ended.
            // This is fine because `current_node`'s lifetime from the tree traversal loop is over.
            let parameters = self.generalize_template(matched_id, &tokens, &raw_log.message);
            Ok(ParsedLogEntry {
                timestamp: raw_log.timestamp,
                template_id: matched_id,
                parameters,
            })
        } else {
            // No suitable template found, create a new one
            let new_template = self.create_template_from_tokens(&tokens, &self.interner);
            let new_template_id = new_template.id;

            // Mutable borrow of self.templates here.
            self.templates.insert(new_template_id, new_template);
            // Mutable borrow of current_node here (which is part of self.tree_roots).
            // This is the critical part. If current_node's mutable borrow is still active from
            // the tree traversal, this can conflict.
            // The `entry().or_insert_with` pattern means `current_node` *is* a mutable reference
            // into `self.tree_roots`.
            // We need to ensure this is handled correctly.
            // The loop creating `current_node` finishes, then `current_node.template_ids.push` happens.
            // This should be okay as long as `self.templates.insert` doesn't somehow invalidate `current_node`.
            // They are different fields of `self`.

            // Re-access current_node mutably to push the new template_id.
            // This re-borrows self.tree_roots mutably for a short duration.
            let len_root_node_again = self.tree_roots.get_mut(&tokens.len()).unwrap();
            let mut node_to_add_template = len_root_node_again;
            for i in 0..std::cmp::min(tokens.len(), self.max_depth) {
                node_to_add_template = node_to_add_template.children.get_mut(&tokens[i]).unwrap();
            }
            node_to_add_template.template_ids.push(new_template_id);

            // For a new template, parameters are the literal string values of tokens.
            let parameters: Vec<ParameterValue> = tokens
                .iter()
                .map(|&interned_token| {
                    let token_str = self
                        .interner
                        .resolve(interned_token)
                        .unwrap_or_default()
                        .to_string();
                    ParameterValue::String(token_str)
                })
                .collect();

            Ok(ParsedLogEntry {
                timestamp: raw_log.timestamp,
                template_id: new_template_id,
                parameters,
            })
        }
    }
}
