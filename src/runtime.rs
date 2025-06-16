use crate::core_structures::{LogTemplate, ParameterValue, RawLogEntry, TemplateToken};
use crate::drain_parser::DrainParser; // From Step 3
use chrono::{DateTime, Utc};
use differential_dataflow::input::InputSession;
use differential_dataflow::operators::Consolidate;
use differential_dataflow::Collection;
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
#[allow(unused_imports)]
use timely::dataflow::operators::Inspect;
use uuid::Uuid; // Added for templates_mirror

pub struct LogProcessingEngine {
    worker_thread_handle: Option<std::thread::JoinHandle<()>>, // To keep the worker alive
    log_sender: mpsc::Sender<Option<RawLogEntry>>, // Channel for sending logs to the worker
    // Collections that can be inspected (will require more sophisticated query mechanisms later)
    // These are placeholders to show intent; actual querying will be more complex.
    // For now, we might not store Arc<Mutex<Collection>> directly but build them in the dataflow.
    // The ability to query them will come from dataflow probes or specific arrangements.

    // We need a way to get templates out for the Drain API (collect_log_groups)
    // This might be a shared map updated by a probe on the templates collection.
    templates_mirror: Arc<Mutex<HashMap<Uuid, LogTemplate>>>,
}

impl LogProcessingEngine {
    pub fn new() -> Self {
        let templates_mirror_arc = Arc::new(Mutex::new(HashMap::new()));
        let templates_mirror_handle = templates_mirror_arc.clone();

        let (sender, receiver) = mpsc::channel::<Option<RawLogEntry>>();
        let receiver = Arc::new(Mutex::new(receiver));
        let receiver_handle = receiver.clone();

        let worker_thread_handle = std::thread::spawn(move || {
            // Initialize Timely worker with a single worker thread.
            timely::execute::execute_from_args(std::iter::empty::<String>(), move |worker| {
                let mut drain_parser = DrainParser::new(0.6, 4); // Default params for now
                let templates_mirror_handle_clone_for_dataflow = templates_mirror_handle.clone();
                let mut input_session: InputSession<usize, RawLogEntry, isize> =
                    InputSession::new();

                worker.dataflow::<usize, _, _>(|scope| {
                    // Changed Timestamp to usize
                    // Create an input stream from the InputSession
                    let raw_logs_stream = input_session.to_collection(scope);

                    // --- Parsing Stage ---
                    // Map RawLogEntry to (ParsedLogEntry, Option<LogTemplate>)
                    // Option<LogTemplate> is Some if a new template was created or an existing one significantly changed.
                    let parsed_and_templates = raw_logs_stream.map(move |raw_log_entry| {
                        // For collection map, the input is just the data (RawLogEntry in this case)
                        // The original timely timestamp and diff are handled by DD framework.
                        // We need to associate a diff for the output records.
                        // If process_raw_log is 1-to-1, diff remains 1.
                        // If it can fail, we might need flat_map or to output (data, diff) tuples from the map.
                        // Let's assume diff is 1 for now for successful parses.
                        // The map operation on Collection implies that the diff of the input record
                        // is carried over to the output record.

                        match drain_parser.process_raw_log(&raw_log_entry) {
                            Ok(parsed_log) => {
                                // Try to get the template that was just processed/created by drain_parser
                                // This requires drain_parser to expose a way to get the latest template,
                                // or for process_raw_log to return it.
                                let template =
                                    drain_parser.get_template_by_id(&parsed_log.template_id);

                                // Output: (ParsedLogEntry_data, template_data_option)
                                // ParsedLogEntry_data: (timestamp, template_id, parameters)
                                // template_data_option: Option<(template_id, tokens)>
                                (
                                    (
                                        parsed_log.timestamp,
                                        parsed_log.template_id,
                                        parsed_log.parameters,
                                    ),
                                    template.map(|t| (t.id, t.tokens)),
                                )
                            }
                            Err(_e) => {
                                // To handle errors and maintain the collection structure,
                                // we should output a special value or use flat_map to filter.
                                // For now, outputting a dummy/default value.
                                // This means downstream operators must be able to handle or filter it.
                                // A more robust solution is needed for proper error handling.
                                (
                                    (Utc::now(), Uuid::nil(), Vec::new()), // Dummy ParsedLogEntry data
                                    None,                                  // No template data
                                )
                            }
                        }
                    });

                    // parsed_and_templates is already a Collection here due to raw_logs_stream.map()
                    let parsed_and_templates_collection = parsed_and_templates;
                    // parsed_and_templates_collection is now Collection<G, ((DateTime<Utc>, Uuid, Vec<ParameterValue>), Option<(Uuid, Vec<TemplateToken>)>), isize>

                    // --- Extract ParsedLogEntries for the `parsed_logs` collection ---
                    // The map operator on Collection takes `(data)` and preserves `(time, diff)`
                    let parsed_logs_data =
                        parsed_and_templates_collection.map(|(ple_data, _template_opt)| ple_data);

                    // `parsed_logs` Collection: ((event_timestamp, template_id), parameters)
                    // This is a placeholder structure. It might need to be ((event_timestamp, template_id, unique_event_id), parameters)
                    // if we need to distinguish identical events at the same timestamp from the same template.
                    // For now, ((DateTime<Utc>, Uuid), Vec<ParameterValue>)
                    #[allow(clippy::type_complexity)]
                    let parsed_logs: Collection<
                        _,
                        ((DateTime<Utc>, Uuid), Vec<ParameterValue>),
                        isize,
                    > = parsed_logs_data.map(|(ts, tid, params)| ((ts, tid), params));

                    parsed_logs.consolidate().inspect(|x| {
                        println!(
                            "Parsed Log (Timestamp: {:?}, Data: {:?}, Diff: {:?})",
                            x.1, x.0, x.2
                        )
                    });

                    // --- Extract LogTemplates for the `log_templates` collection ---
                    // Filter out None templates and map to (template_id, tokens)
                    let log_templates_data = parsed_and_templates_collection
                        .flat_map(|(_ple_data, template_opt)| template_opt.into_iter());

                    // `log_templates` Collection: (template_id, tokens)
                    let log_templates: Collection<_, (Uuid, Vec<TemplateToken>), isize> =
                        log_templates_data;

                    // Consolidate to ensure templates are unique if multiple identical templates are emitted.
                    // Then, use a probe to update the shared templates_mirror.
                    let templates_mirror_handle_for_inspect =
                        templates_mirror_handle_clone_for_dataflow.clone();
                    log_templates.consolidate().inspect(move |x| {
                        // x is ((data, time, diff)) where data is (Uuid, Vec<TemplateToken>)
                        // Update the shared map for external access
                        let template_id = (x.0).0;
                        let template_tokens = (x.0).1.clone();
                        let diff = x.2;

                        let mut mirror = templates_mirror_handle_for_inspect.lock().unwrap();
                        if diff > 0 {
                            mirror.insert(
                                template_id,
                                LogTemplate {
                                    id: template_id,
                                    tokens: template_tokens,
                                },
                            );
                        } else {
                            // Handle retraction if necessary, though simple DRAIN might not retract templates often.
                            // For now, positive diffs add/update.
                        }
                        println!(
                            "Log Template (Timestamp: {:?}, ID: {}, Diff: {:?})",
                            x.1, template_id, diff
                        );
                    });
                });

                let mut time = 0usize;
                loop {
                    let msg = receiver_handle.lock().unwrap().recv();
                    match msg {
                        Ok(Some(raw_log)) => {
                            input_session.update_at(raw_log, time, 1);
                            time += 1;
                            input_session.advance_to(time);
                            input_session.flush();
                            worker.step();
                        }
                        Ok(None) | Err(_) => break,
                    }
                }
            })
            .unwrap(); // timely::execute
        }); // std::thread::spawn

        Self {
            worker_thread_handle: Some(worker_thread_handle),
            log_sender: sender,
            templates_mirror: templates_mirror_arc,
        }
    }

    pub fn ingest_raw_log(&self, raw_log: RawLogEntry) {
        // Send the log entry to the worker thread. Ignore errors during shutdown.
        let _ = self.log_sender.send(Some(raw_log));
    }

    // Placeholder for getting templates (e.g., for Drain::collect_log_groups)
    pub fn get_all_templates(&self) -> Vec<LogTemplate> {
        self.templates_mirror
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    // Placeholder for direct query (will be replaced by proper dataflow queries)
    // pub fn query_parsed_logs_count(&self) -> usize {
    //    // This would require a probe on the parsed_logs collection or other mechanisms
    //    0
    // }
}

impl Default for LogProcessingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LogProcessingEngine {
    fn drop(&mut self) {
        // Signal the worker thread to finish and join it.
        let _ = self.log_sender.send(None);
        if let Some(handle) = self.worker_thread_handle.take() {
            handle.join().expect("Failed to join timely worker thread.");
        }
    }
}
