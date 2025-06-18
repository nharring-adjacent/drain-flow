use crate::drains::dd_types::{LogTemplate, RawLog, TokenizedLog};
use chrono::Duration;
use differential_dataflow::operators::{Consolidate, Iterate, Join, Reduce, Threshold};
use differential_dataflow::Collection;
use lasso::{Spur, ThreadedRodeo};
use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use timely::communication::allocator::generic::Generic;
use timely::dataflow::operators::input::Handle;
use timely::dataflow::operators::{
    Concat, Enter, Exchange, Filter, LoopVariable, Map, Probe, ProbeSupport,
};
use timely::dataflow::scopes::Scope;
use timely::dataflow::ProbeHandle;
use timely::order::Product;
use timely::worker::Worker;
use uuid::Uuid;

lazy_static! {
    // Regex for tokenizing log lines, splitting by delimiters and whitespace,
    // but keeping structured parameters like "param1=value1" or "<param2>" intact.
    // It aims to identify segments that are likely parameters or fixed tokens.
    static ref TOKEN_RE: Regex = Regex::new(r#"(<[^ ]*?>)|([^\s=,;:"'`(){}\[\]]+=[^\s=,;:"'`(){}\[\]]+)|([\w.-]+)|([^\s\w])"#).unwrap();
}

const WILDCARD_STR: &str = "<*>"; // Used as a placeholder for variable parts of a log message.

// Function to tokenize a log line using the TOKEN_RE regex.
// Each token is interned using Lasso's ThreadedRodeo for efficient string management.
fn tokenize_log_line(line: &str, interner: &Arc<ThreadedRodeo>) -> Vec<Spur> {
    TOKEN_RE
        .find_iter(line)
        .map(|mat| interner.get_or_intern(mat.as_str()))
        .collect()
}

pub fn build_dataflow_graph_and_get_handles(
    worker: &mut Worker<Generic>,
) -> Result<
    (
        Handle<Product<Duration, u64>, RawLog>,
        ProbeHandle<Product<Duration, u64>>,
    ),
    String,
> {
    let interner_for_dataflow: Arc<ThreadedRodeo> = Arc::new(ThreadedRodeo::default());

    worker
        .dataflow(move |scope| {
            let interner_for_dataflow_clone = Arc::clone(&interner_for_dataflow);
            let (input_handle, stream) = scope.new_input::<RawLog>();

            let interner_for_map = Arc::clone(&interner_for_dataflow_clone);
            let tokenized_logs = stream.map(move |raw_log: RawLog| {
                let tokens = tokenize_log_line(&raw_log.content, &interner_for_map);
                let token_count = tokens.len() as u32; // Ensure token_count is u32
                TokenizedLog {
                    original_id: Uuid::new_v4(), // Generate a unique ID for each log
                    tokens,
                    token_count, // Store the number of tokens
                }
            });

            let (summary_templates, _final_unclustered_optional) =
                scope.iterative::<u32, _, _>(|inner_scope| {
                    let unclustered_initial = tokenized_logs.enter(inner_scope);
                    let templates_initial = Collection::<_, LogTemplate>::new(inner_scope); // Initialize empty templates

                    let (templates_handle, templates_stream) =
                        inner_scope.loop_variable(Product::new(Default::default(), 1));
                    let (unclustered_handle, unclustered_stream) =
                        inner_scope.loop_variable(Product::new(Default::default(), 1));

                    let templates = templates_initial.concat(&templates_stream);
                    let unclustered_logs = unclustered_initial.concat(&unclustered_stream);

                    // A. Filter by Log Length
                    // Group logs by their token count.
                    // Templates are also associated with a specific token count.
                    // This ensures that a log is only compared with templates of the same length.
                    let logs_by_length = unclustered_logs.map(|log| (log.token_count, log));
                    let templates_by_length =
                        templates.map(|template| (template.token_count, template));

                    // B. Calculate Similarity Score
                    // Join logs with templates of the same length.
                    // For each (log, template) pair, calculate their similarity.
                    // The similarity is defined as the number of matching tokens (excluding wildcards in the template)
                    // divided by the total number of tokens (which is the same for log and template due to pre-filtering).
                    const SIMILARITY_THRESHOLD: f64 = 0.6; // Configurable threshold for matching
                    let interner_for_similarity = Arc::clone(&interner_for_dataflow_clone);
                    let potential_matches = logs_by_length
                        .join_map(&templates_by_length, move |&_len, log, template| {
                            let mut matching_tokens = 0;
                            let wildcard_spur =
                                interner_for_similarity.get_or_intern_static(WILDCARD_STR);
                            for (log_token, template_token) in
                                log.tokens.iter().zip(template.token_spurs.iter())
                            {
                                if *template_token != wildcard_spur && log_token == template_token {
                                    matching_tokens += 1;
                                }
                            }
                            let similarity = matching_tokens as f64 / log.token_count as f64;
                            (log.clone(), template.clone(), similarity)
                        })
                        .filter(move |_log, _template, similarity| {
                            *similarity >= SIMILARITY_THRESHOLD
                        });

                    // C. Select Best Matching Template
                    // A log might match multiple templates. We need to select the one with the highest similarity.
                    // If there are ties, the choice is arbitrary but consistent due to differential dataflow's determinism.
                    // (log, (template, similarity))
                    let best_match = potential_matches
                        .map(|(log, template, similarity)| {
                            (log.original_id, (log, template, similarity))
                        })
                        .reduce(|_log_id, group_iter, output| {
                            // Find the best match within the group
                            let mut best_similarity = -1.0;
                            let mut best_entry = None;
                            for (log, template, similarity) in group_iter {
                                if *similarity > best_similarity {
                                    best_similarity = *similarity;
                                    best_entry = Some((log.clone(), template.clone()));
                                }
                            }
                            if let Some(entry) = best_entry {
                                output.push((entry, 1));
                            }
                        });

                    let matched_logs = best_match.map(|((log, _template), _)| log);
                    let successfully_matched_templates =
                        best_match.map(|((_log, template), _)| template);

                    // Logs that didn't find a match remain unclustered for the next iteration or become new templates.
                    let unmatched_logs_feedback = unclustered_logs
                        .map(|log| (log.original_id, log))
                        .antijoin(&matched_logs.map(|log| log.original_id))
                        .map(|(_id, log)| log);

                    // D. Generalize Templates Based on Matched Logs (This step is simplified here)
                    // The original DRAIN paper describes a more complex generalization.
                    // Here, we'll just pass the templates that successfully matched.
                    // A more complete implementation would update template tokens to wildcards based on variance.
                    // For this example, we assume templates are either static or this generalization is minimal.
                    // The key is that `updated_templates` are fed back.
                    // let interner_for_generalization = Arc::clone(&interner_for_dataflow_clone);
                    let updated_templates = successfully_matched_templates; // Simplified: templates that matched are kept.

                    // E. Create New Templates from Unmatched Logs
                    // Logs that couldn't be matched to any existing template become seeds for new templates.
                    // Their tokens are used directly to form a new LogTemplate.
                    let new_templates_from_unmatched = unmatched_logs_feedback.map(|log| {
                        LogTemplate {
                            template_id: Uuid::new_v4(), // Each new template gets a unique ID
                            token_spurs: log.tokens,
                            token_count: log.token_count,
                            // initial_log_ids: vec![log.original_id], // Optional: track which logs formed this template
                        }
                    });

                    // Combine updated templates with newly created templates for the next iteration.
                    let next_iteration_templates =
                        updated_templates.concat(&new_templates_from_unmatched);

                    templates_handle.set(next_iteration_templates.consolidate());
                    unclustered_handle
                        .set(unmatched_logs_feedback.map(|l| l.clone()).consolidate()); // Ensure log is cloned for feedback

                    (templates.leave(), Some(unclustered_logs.leave()))
                });

            let final_probe = summary_templates.probe();
            Ok((input_handle, final_probe))
        })
        .map_err(|e| e.to_string())
        .and_then(|res| res)
}
