use crate::drains::dd_types::{LogTemplate, RawLog, TokenizedLog};
use chrono::Duration;
// use differential_dataflow::input::InputSession;
// Group, Antijoin, and Concat are typically used as trait methods on Collection objects
// and may not need to be (or cannot be) imported directly from this path.
// Other specific modules like `differential_dataflow::operators::set::Antijoin` exist if needed,
// but the methods themselves are usually brought in by `use differential_dataflow::operators::*;` or traits.
use differential_dataflow::operators::{Consolidate, Iterate, Join, Reduce, Threshold};
use differential_dataflow::Collection; // Required for .concat(), .antijoin(), .group() trait methods
use lasso::{Spur, ThreadedRodeo}; // Ensure ThreadedRodeo is imported
use lazy_static::lazy_static;
use regex::Regex;
use std::sync::mpsc::channel;
use std::sync::Arc;
use timely::dataflow::operators::input::Handle;
use timely::dataflow::operators::{Concat, Enter, Input, Leave, LoopVariable, Map, Probe};
use timely::dataflow::scopes::Scope;
use timely::dataflow::ProbeHandle;
// use timely::execute;
use timely::order::Product;
// use timely::progress::Timestamp;
use uuid::Uuid;

/// WILDCARD_STR is a constant string used to represent a wildcard token in log templates.
const WILDCARD_STR: &str = "<*>";

lazy_static! {
    /// TOKEN_RE is a regular expression used to tokenize log lines.
    /// It identifies IP addresses, UUIDs, numbers, delimiters, words, and other non-whitespace characters.
    static ref TOKEN_RE: Regex = Regex::new(
        r#"(?x)
        (\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}) | # IP Addresses
        ([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}) | # UUIDs
        (\d+\.\d+|\d+) | # Numbers (float or int)
        ([=():\[\]{}<>]) | # Delimiters
        ([\w-]+) | # Words (alphanumeric, hyphen, underscore)
        (\S) # Any other non-whitespace character
    "#
    )
    .unwrap();
}

/// Tokenizes a log line using the global `TOKEN_RE` and interns the resulting tokens
/// using the provided `ThreadedRodeo` interner.
/// Returns a vector of `Spur`s, which are interned string identifiers.
fn tokenize_log_line(line: &str, interner: &ThreadedRodeo) -> Vec<Spur> {
    TOKEN_RE
        .find_iter(line)
        .map(|mat| interner.get_or_intern(mat.as_str()))
        .collect()
}

pub struct DifferentialDrainRuntime {
    input_handle: Option<Handle<timely::order::Product<Duration, u64>, RawLog>>,
    probe: ProbeHandle<timely::order::Product<Duration, u64>>,
    lasso_interner: Arc<ThreadedRodeo>,
}

impl DifferentialDrainRuntime {
    pub fn new() -> Result<Self, String> {
        // Initialize a string interner (Lasso's ThreadedRodeo) wrapped in an Arc for shared access.
        let initial_lasso_interner: Arc<ThreadedRodeo> = Arc::new(ThreadedRodeo::default());
        // Create channels to send dataflow handles from the timely worker thread to the main thread.
        let (input_handle_sender, input_handle_receiver) = channel();
        let (probe_sender, probe_receiver) = channel();

        // Clone the interner Arc for the new timely worker thread.
        let interner_for_thread: Arc<ThreadedRodeo> = Arc::clone(&initial_lasso_interner);

        // Spawn a new thread to host the Timely Dataflow computation.
        std::thread::spawn(move || {
            // Execute a Timely Dataflow computation.
            // `timely::Configuration::Thread` indicates a single-worker execution.
            if let Err(e) = timely::execute::execute(
                timely::execute::Config::thread(),
                move |worker| {
                    // Clone the interner Arc again for the dataflow construction closure.
                    // This interner (`interner_for_dataflow`) will be moved into the main dataflow scope
                    // and subsequently cloned for specific operators needing access to it.
                    let interner_for_dataflow: Arc<ThreadedRodeo> =
                        Arc::clone(&interner_for_thread);

                    // Define the dataflow graph.
                    let (input_h, probe_h) = worker.dataflow(move |scope| {
                    // === Input and Tokenization ===
                    // Create a new input stream for RawLog messages.
                    // `input_handle` is used to send data into the stream from outside the dataflow.
                    // `stream` is the Collection of RawLog messages within the dataflow.
                    let (input_handle, stream) = scope.new_input::<RawLog>();

                    // Clone the interner for the tokenization map operation.
                    let interner_for_map: Arc<ThreadedRodeo> = Arc::clone(&interner_for_dataflow);
                    // Map RawLog messages to TokenizedLog messages.
                    // This involves tokenizing the log text and assigning a unique ID.
                    let tokenized_logs = stream.map(move |raw_log: RawLog| {
                        let tokens = tokenize_log_line(&raw_log.content, &interner_for_map);
                        let token_count = tokens.len();
                        TokenizedLog {
                            original_id: Uuid::new_v4(), // Assign a unique ID for tracking this specific log instance.
                            tokens,
                            token_count,
                        }
                    });

                    // === Iterative Log Clustering (DRAIN algorithm) ===
                    // `scope.iterative` defines a dataflow region that can loop back on itself.
                    // It's used here to iteratively refine log templates.
                    // The type parameter `u32` is for the iteration counter.
                    let (summary_templates, _final_unclustered_optional) = scope.iterative::<u32, _, _>(move |inner_scope| {
                        // Bring the stream of tokenized logs into the iterative scope.
                        let unclustered_initial = tokenized_logs.enter(inner_scope);
                        // Initialize an empty collection for log templates at the beginning of the first iteration.
                        let templates_initial = Collection::<_, LogTemplate>::new(inner_scope);

                        // Define loop variables for templates and unclustered logs.
                        // `templates_handle` sends data back into the `templates_stream` for the next iteration.
                        // `unclustered_handle` sends data back into the `unclustered_stream` for the next iteration.
                        let (templates_handle, templates_stream) = inner_scope.loop_variable(Product::new(Default::default(), 1));
                        let (unclustered_handle, unclustered_stream) = inner_scope.loop_variable(Product::new(Default::default(), 1));

                        // Combine initial inputs with feedback from previous iterations.
                        // `templates` = initial templates (empty at first) + templates fed back from the previous iteration.
                        let templates = templates_initial.concat(&templates_stream);
                        // `unclustered_logs` = initial tokenized logs + logs fed back as unclustered from the previous iteration.
                        let unclustered_logs = unclustered_initial.concat(&unclustered_stream);

                        // --- Start of DRAIN Core Logic ---

                        // A. Join Unclustered Logs with Existing Templates
                        // Key logs and templates by their `token_count` for an efficient join.
                        // Only logs and templates with the same number of tokens can be candidates for matching.
                        let current_unclustered_keyed = unclustered_logs.map(|log: TokenizedLog| (log.token_count, log.clone()));
                        let current_templates_keyed = templates.map(|tpl: LogTemplate| (tpl.token_count, tpl.clone()));

                        // Perform the join. Output is `(TokenizedLog, LogTemplate)` for pairs with matching token counts.
                        let joined_logs_templates = current_unclustered_keyed
                            .join_core(&current_templates_keyed, |&_token_count, log, template| {
                                Some((log.clone(), template.clone()))
                            });
                        // Collection<_, (TokenizedLog, LogTemplate)>

                        // B. Calculate Similarity Score
                        // `SIMILARITY_THRESHOLD` determines how similar a log must be to a template to be considered a match.
                        const SIMILARITY_THRESHOLD: f64 = 0.6;
                        // Clone interner for use in the similarity calculation closure.
                        // `interner_for_dataflow` (moved into `iterative` scope) is cloned here.
                        let interner_for_similarity: Arc<ThreadedRodeo> = Arc::clone(&interner_for_dataflow);

                        // `flat_map` processes each (log, template) pair to calculate a similarity score.
                        // If the score is above the threshold, it emits `(log_original_id, (template_id, score_as_u32))`.
                        let potential_matches = joined_logs_templates.flat_map(move |(log, template)| {
                            // Resolve the wildcard string to its interned Spur representation.
                            let wildcard_spur = interner_for_similarity.get_or_intern(WILDCARD_STR);
                            let mut matching_tokens = 0;
                            // Compare tokens: a log token matches a template token if they are identical
                            // or if the template token is a wildcard.
                            for (log_token, template_token) in log.tokens.iter().zip(template.template_tokens.iter()) {
                                if *log_token == *template_token || *template_token == wildcard_spur {
                                    matching_tokens += 1;
                                }
                            }
                            // Calculate similarity score.
                            let score = if log.token_count > 0 {
                                matching_tokens as f64 / log.token_count as f64
                            } else if template.token_count == 0 { // Both empty
                                1.0
                            } else { // Log empty, template not (or vice-versa if not caught by token_count join)
                                0.0
                            };

                            if score >= SIMILARITY_THRESHOLD {
                                // Emit if score is high enough. Score is scaled to u32 for easier reduction later.
                                Some((log.original_id, (template.template_id, (score * 1000.0) as u32)))
                            } else {
                                None // Otherwise, no match from this pair.
                            }
                        });
                        // Collection<_, (log_original_id: Uuid, (template_id: Uuid, score: u32))>

                        // C. Find Best Match for Each Log
                        // For logs that matched multiple templates, select the one with the highest score.
                        let best_match_for_logs = potential_matches
                            .map(|(log_id, (template_id, score))| (log_id, (score, template_id))) // Key by log_id, value (score, template_id) for max_by_key
                            .reduce(|_log_id, inputs, output| { // Group by log_id
                                // `inputs` contains all ((score, template_id), multiplicity) for a given log_id.
                                // Find the entry with the maximum score.
                                if let Some(max_entry) = inputs.iter().filter(|(_val, diff)| *diff > 0) // Consider only positive contributions
                                    .max_by_key(|&(&(score, _template_id), _diff)| score) {
                                    // `max_entry.0` is `(score, template_id)`.
                                    output.push((max_entry.0.clone(), 1)); // Emit the (max_score, template_id) with multiplicity 1.
                                }
                            })
                            .map(|(log_id, (_score, template_id))| (log_id, template_id)); // Discard score, keep (log_id, best_template_id).
                        // Collection<_, (log_original_id: Uuid, best_template_id: Uuid)>

                        // D. Generalize Templates Based on Matched Logs
                        // Prepare data for generalization: we need (template_id, TokenizedLog that matched it).
                        let logs_for_generalization = best_match_for_logs
                            .map(|(log_id, template_id)| (log_id, template_id))
                            .join(&unclustered_logs.map(|log| (log.original_id, log.clone()))) // Join with original TokenizedLog
                            .map(|(_log_id, template_id, log)| (template_id, log));
                        // Collection<_, (template_id, TokenizedLog)>

                        // Further prepare: we need (template_id, TokenizedLog, old_template_details)
                        let data_for_grouping = logs_for_generalization
                            .join(&templates.map(|tpl| (tpl.template_id, tpl.clone()))) // Join with current templates
                            .map(|(template_id, tokenized_log, old_template)| (template_id, tokenized_log, old_template));
                        // Collection<_, (template_id, TokenizedLog, LogTemplate)>

                        // Clone interner for the generalization group closure.
                        // `interner_for_dataflow` (moved into `iterative` scope) is cloned here.
                        let interner_for_generalization: Arc<ThreadedRodeo> = Arc::clone(&interner_for_dataflow);
                        // Group by `template_id` to generalize each template with all logs that matched it.
                        let updated_templates = data_for_grouping
                            .map(|(template_id, log, original_template)|
                                // Prepare value for grouping: (log_tokens, original_template_tokens, original_token_count)
                                (template_id, (log.tokens, original_template.template_tokens, original_template.token_count))
                            )
                            .group(move |template_id_key, inputs, output| { // `template_id_key` is the template_id being processed.
                                // `inputs` is &[(&(log_tokens, original_template_tokens, original_token_count), multiplicity)]
                                if inputs.is_empty() { return; }

                                // All `original_template_tokens` in this group for this `template_id_key` are identical.
                                // Use the first one as the base for generalization.
                                let mut generalized_tokens = inputs[0].0.1.clone();
                                let original_token_count = inputs[0].0.2;

                                if generalized_tokens.len() != original_token_count {
                                     eprintln!("Warning: Template token length mismatch during generalization for template ID: {:?}", template_id_key);
                                }
                                // Get the interned wildcard spur.
                                let wildcard_spur = interner_for_generalization.get_or_intern(WILDCARD_STR);

                                // Iterate over each log that matched this template.
                                for ((log_tokens, _original_template_tokens, _), diff) in inputs.iter() {
                                    if *diff <= 0 { continue; } // Process only positive contributions.

                                    let len_to_compare = std::cmp::min(generalized_tokens.len(), log_tokens.len());
                                    // Compare tokens one by one. If a log token differs from the current generalized token,
                                    // and the generalized token is not already a wildcard, make it a wildcard.
                                    for i in 0..len_to_compare {
                                        if generalized_tokens[i] != wildcard_spur && generalized_tokens[i] != log_tokens[i] {
                                            generalized_tokens[i] = wildcard_spur;
                                        }
                                    }
                                }
                                // Emit the (potentially) generalized template.
                                output.push((LogTemplate {
                                    template_id: *template_id_key, // The ID of the template being updated.
                                    template_tokens: generalized_tokens,
                                    token_count: original_token_count, // Token count of the template does not change.
                                }, 1)); // Multiplicity 1 for the updated template.
                            });
                        // Collection<_, LogTemplate> (generalized templates)

                        // E. Feedback Logic: Determine which logs remain unclustered and form new templates.

                        // Identify IDs of logs that were successfully clustered in this iteration.
                        let clustered_log_ids_keys = best_match_for_logs
                            .map(|(log_id, _template_id)| log_id) // Get log_id from (log_id, best_template_id)
                            .distinct_core(); // Get unique log IDs. Produces Collection<G, Uuid>

                        // Find logs that were not matched to any template using antijoin.
                        // These are the `unclustered_logs` from the start of this iteration minus `clustered_log_ids_keys`.
                        let unmatched_logs_feedback = unclustered_logs
                            .map(|log| (log.original_id, log.clone())) // Key by original_id
                            .antijoin(&clustered_log_ids_keys)      // Subtract logs that were clustered.
                            .map(|(_log_id, log)| log);             // Keep the TokenizedLog object.
                        // Collection<_, TokenizedLog>

                        // Create new templates from these unmatched logs.
                        // Each unmatched log forms a new template with its exact tokens.
                        let new_templates_from_unmatched = unmatched_logs_feedback.map(|log: TokenizedLog| {
                            LogTemplate {
                                template_id: Uuid::new_v4(), // Generate a new unique ID for this new template.
                                template_tokens: log.tokens.clone(),
                                token_count: log.token_count,
                            }
                        });
                        // Collection<_, LogTemplate>

                        // Combine generalized templates with newly created templates.
                        // This forms the complete set of templates to be used in the next iteration.
                        let next_iteration_templates = updated_templates.concat(&new_templates_from_unmatched);

                        // Feed the combined templates back into the `templates` loop variable.
                        // `consolidate()` ensures that multiplicities are correctly handled for diffs.
                        let pinned_loop_handle = std::pin::pin!(templates_handle);
                         pinned_loop_handle.set(next_iteration_templates.consolidate( ));

                        // Feed the logs that remained unmatched in this iteration back into the `unclustered_logs` loop variable.
                        let pinned_cluster_handle = std::pin::pin!(unclustered_handle);
                        pinned_cluster_handle.set(unmatched_logs_feedback.consolidate());

                        // Output of the iterative scope:
                        // `templates.leave()`: The full collection of templates accumulated over all iterations.
                        // `unclustered_logs.leave()`: Logs that remained unclustered after the process stabilized.
                        (templates.leave(), Some(unclustered_logs.leave()))
                    });

                    // Probe the final collection of summary templates.
                    let final_probe = summary_templates.probe();
                    (input_handle, final_probe) // Return input handle and probe handle to the caller of `worker.dataflow`.
                });

                    // Send the handles out of the worker thread.
                    if input_handle_sender.send(input_h).is_err() {
                        eprintln!("Failed to send input handle: receiver dropped");
                    }
                    if probe_sender.send(probe_h).is_err() {
                        eprintln!("Failed to send probe handle: receiver dropped");
                    }
                },
            ) {
                eprintln!("Timely worker execution failed: {:?}", e);
            }
        });

        // Receive handles from the worker thread.
        let input_handle = input_handle_receiver
            .recv()
            .map_err(|e| format!("Failed to receive input handle from worker thread: {}", e))?;
        let probe = probe_receiver
            .recv()
            .map_err(|e| format!("Failed to receive probe handle from worker thread: {}", e))?;

        // Construct and return the DifferentialDrainRuntime instance.
        Ok(DifferentialDrainRuntime {
            input_handle: Some(input_handle),
            probe,
            lasso_interner: initial_lasso_interner, // Store the original interner Arc.
        })
    }
}

/*
impl DifferentialLogBundle<Runtime> for DifferentialDrainRuntime {
    fn flush_blackhole(&mut self) {
        // TODO: Implement blackhole flushing.
    }

    fn flush_materialize(&mut self) {
        // TODO: Implement materialize flushing.
    }
}
*/
