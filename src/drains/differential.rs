use std::collections::HashMap; 
use std::sync::Arc;

use differential_dataflow::input::InputSession;
use differential_dataflow::operators::iterate::Variable;
// Corrected DD operator imports; Collection is top-level, Map removed (using timely's Map)
use differential_dataflow::{Collection, operators::{Join, Reduce, Consolidate}}; 
use lazy_static::lazy_static;
use parking_lot::RwLock;
use string_interner::StringInterner;
use timely::dataflow::scopes::Child;
use timely::dataflow::operators::{Filter, Map}; // Added timely::dataflow::operators::Map
use timely::communication::allocator::Allocate;
use timely::worker::Worker;
use timely::progress::Timestamp;
use timely::progress::timestamp::RootTimestamp; // Verified this path
use uuid::Uuid;
// Abomonation import removed.
// Rkyv imports removed.
use serde::{Serialize, Deserialize}; // Added serde imports

use crate::record::Record;

lazy_static! {
    pub(crate) static ref INTERNER: Arc<RwLock<StringInterner>> =
        Arc::new(RwLock::new(StringInterner::default()));
}

// Enum to represent different types of aggregations for log fields
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum FieldAggregation {
    Sum(u64),
    Average(f64), // TODO: Implement robust Average aggregation
    DistinctValues(Vec<String>),
}

// Represents the template of a log group, with potential wildcards.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct LogTemplate {
    // #[with(uuid_rkyv_adapter::UuidAsBytes)] // Removed rkyv attribute
    pub id: Uuid, // Uuid with "serde" feature should work directly
    pub tokens: Vec<Option<String>>, 
}

// Data associated with each log event/record that's processed.
#[derive(Debug, Clone)]
pub struct MatchedLogEvent<TS: Timestamp> { // TS for Timestamp
    template: LogTemplate, 
    timestamp: TS,
    params: Vec<(String, ExtractedParamValue)>, 
}

#[derive(Debug, Clone)]
pub enum ExtractedParamValue {
    Numeric(f64),
    Text(String),
}

// State maintained for each active log template in the iteration variable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateState<TS: Timestamp> { 
    template: LogTemplate,
    count: u64,
    first_seen_timestamp: TS,
    last_updated_timestamp: TS,
    aggregated_fields: HashMap<String, FieldAggregation>,
}

// Structure to represent a log group in Differential Dataflow (output)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] 
pub struct LogGroupDD<TS: Timestamp> { 
    // #[with(uuid_rkyv_adapter::UuidAsBytes)] // Removed rkyv attribute
    pub template_id: Uuid, // Uuid with "serde" feature should work directly
    pub template_str: String,
    pub count: u64,
    pub first_seen_timestamp: TS,
    pub last_updated_timestamp: TS,
    pub rate: f64,
    pub rate_of_change: f64, // TODO: Implement rate of change calculation
    pub aggregated_fields: HashMap<String, FieldAggregation>,
    pub internal_template_tokens: Vec<Option<String>>, 
}

// uuid_rkyv_adapter module removed.

fn generate_template_tokens_from_record(record: &Record) -> Vec<Option<String>> {
    let interner = INTERNER.read();
    record
        .tokens()
        .iter()
        .map(|token_sym| {
            let token_str = interner.resolve(*token_sym).unwrap_or("").to_string();
            if token_str.chars().all(char::is_numeric) {
                None
            } else {
                Some(token_str)
            }
        })
        .collect()
}

fn extract_parameters_from_record<TS: Timestamp>( // TS for Timestamp
    record: &Record,
    matched_template: &LogTemplate,
    timestamp: TS,
) -> MatchedLogEvent<TS> {
    let mut params = Vec::new();
    let interner = INTERNER.read();
    let record_raw_tokens = record.tokens();

    for (i, template_token_opt) in matched_template.tokens.iter().enumerate() {
        if template_token_opt.is_none() && i < record_raw_tokens.len() {
            // This position is a wildcard in the template, and the record has a token here.
            let param_val_str = interner
                .resolve(record_raw_tokens[i])
                .unwrap_or("")
                .to_string();
            let param_name = format!("param_{}", i);

            if let Ok(num_val) = param_val_str.parse::<f64>() {
                params.push((param_name, ExtractedParamValue::Numeric(num_val)));
            } else {
                params.push((param_name, ExtractedParamValue::Text(param_val_str)));
            }
        }
    }
    MatchedLogEvent {
        template: matched_template.clone(),
        timestamp,
        params,
    }
}

fn format_template_tokens(tokens: &[Option<String>]) -> String {
    tokens
        .iter()
        .map(|opt_token| opt_token.as_deref().unwrap_or("<*>"))
        .collect::<Vec<_>>()
        .join(" ")
}

// TS is the Timestamp type, A is the Allocator type for the worker.
pub struct DifferentialDrain<'a, A, TS> 
where 
    A: Allocate,
    TS: Timestamp + Clone + Ord + Send + 'static + serde::Serialize + for<'de> serde::Deserialize<'de>,
{
    raw_log_line_input: InputSession<TS, (String, TS), isize>,
    pub log_groups_collection: Collection<Child<'a, Worker<A>, TS>, LogGroupDD<TS>, isize>,
}

impl<'a, A, TS> DifferentialDrain<'a, A, TS>
where
    A: Allocate,
    TS: Timestamp + Clone + Ord + Send + 'static + serde::Serialize + for<'de> serde::Deserialize<'de>,
    Record: serde::Serialize + for<'de> serde::Deserialize<'de> + Clone + Send + 'static,
{
    pub fn new(scope: &mut Child<'a, Worker<A>, TS>) -> Self { // Scope is already correctly Child<'a, Worker<A>, TS>
        let (raw_log_line_input, raw_log_lines_with_ts) = scope.new_collection::<(String, TS), isize>(); // Confirmed TS
        let tokenized_records_with_ts = raw_log_lines_with_ts.map(|(line, ts)| (Record::new(line), ts));
        let similarity_threshold = 0.6;

        let final_log_groups = scope.iterate(|inner_scope| {
            let templates_state_var = Variable::new_from(
                Collection::new(inner_scope.parent()).as_collection(),
                1,
            );
            let tokenized_records_entered = inner_scope.enter(&tokenized_records_with_ts);

            let record_and_match_state = tokenized_records_entered.join_map_u(
                &templates_state_var.map(|(_id, state)| state),
                |(record, _original_ts), template_state| {
                    let record_template_tokens = generate_template_tokens_from_record(record);
                    if record_template_tokens.len() != template_state.template.tokens.len() {
                        return None;
                    }
                    let mut matching_tokens = 0;
                    let mut template_concrete_tokens = 0;
                    for (rec_tok_opt, tmpl_tok_opt) in record_template_tokens.iter().zip(template_state.template.tokens.iter()) {
                        if tmpl_tok_opt.is_some() { template_concrete_tokens += 1; if rec_tok_opt == tmpl_tok_opt { matching_tokens += 1; }}
                    }
                    let similarity = if template_concrete_tokens == 0 { if record_template_tokens.is_empty() { 1.0 } else { 0.0 } } else { matching_tokens as f64 / template_concrete_tokens as f64 };
                    if similarity >= similarity_threshold { Some(template_state.clone()) } else { None }
                },
            );

            let matched_events = record_and_match_state
                .filter_map(|((record, original_ts), opt_state)| {
                    opt_state.map(|state| extract_parameters_from_record(&record, &state.template, original_ts))
                });

            let unmatched_records_with_ts = record_and_match_state
                .filter_map(|((record, original_ts), opt_state)| {
                    if opt_state.is_none() { Some((record, original_ts)) } else { None }
                });

            let new_template_states = unmatched_records_with_ts.map(|(record, ts)| {
                let template_tokens = generate_template_tokens_from_record(&record);
                let new_template = LogTemplate { id: Uuid::new_v4(), tokens: template_tokens };
                let params_for_new = extract_parameters_from_record(&record, &new_template, ts.clone()).params;
                let mut initial_aggr = HashMap::new();
                for (param_name, param_val) in params_for_new {
                    match param_val {
                        ExtractedParamValue::Numeric(n) => { initial_aggr.insert(param_name.clone(), FieldAggregation::Sum(n as u64)); }
                        ExtractedParamValue::Text(t) => { initial_aggr.insert(param_name.clone(), FieldAggregation::DistinctValues(vec![t])); }
                    }
                }
                let new_state = TemplateState {
                    template: new_template.clone(),
                    count: 1,
                    first_seen_timestamp: ts.clone(),
                    last_updated_timestamp: ts,
                    aggregated_fields: initial_aggr,
                };
                (new_template.id, new_state)
            });

            let single_event_states = matched_events.map(|event| {
                let mut event_aggr = HashMap::new();
                for (param_name, param_val) in event.params {
                    match param_val {
                        ExtractedParamValue::Numeric(n) => { event_aggr.insert(param_name.clone(), FieldAggregation::Sum(n as u64)); }
                        ExtractedParamValue::Text(t) => { event_aggr.insert(param_name.clone(), FieldAggregation::DistinctValues(vec![t])); }
                    }
                }
                (event.template.id, TemplateState {
                    template: event.template.clone(),
                    count: 1,
                    first_seen_timestamp: event.timestamp.clone(),
                    last_updated_timestamp: event.timestamp,
                    aggregated_fields: event_aggr,
                })
            });

            let all_state_inputs = templates_state_var
                .as_collection()
                .concat(&new_template_states)
                .concat(&single_event_states);

            let feedback = all_state_inputs.reduce_u(move |_template_id_key, inputs, output| {
                let mut merged_state: Option<TemplateState<TS>> = None; // Confirmed TS
                for (_time, val_diff_tuple) in inputs {
                    let current_event_state = &val_diff_tuple.0; 
                    if merged_state.is_none() {
                        merged_state = Some(current_event_state.clone());
                    } else {
                        let ms = merged_state.as_mut().unwrap();
                        ms.count += current_event_state.count;
                        if current_event_state.first_seen_timestamp < ms.first_seen_timestamp {
                            ms.first_seen_timestamp = current_event_state.first_seen_timestamp.clone();
                        }
                        if current_event_state.last_updated_timestamp > ms.last_updated_timestamp {
                            ms.last_updated_timestamp = current_event_state.last_updated_timestamp.clone();
                        }
                        for (param_name, agg_val) in current_event_state.aggregated_fields.iter() {
                            let entry = ms.aggregated_fields.entry(param_name.clone()).or_insert_with(|| agg_val.clone());
                            match (entry, agg_val) {
                                (FieldAggregation::Sum(ref mut s_curr), FieldAggregation::Sum(s_new)) => {
                                    if !Arc::ptr_eq(s_curr, s_new) { *s_curr += s_new; }
                                }
                                (FieldAggregation::DistinctValues(ref mut v_curr), FieldAggregation::DistinctValues(v_new)) => {
                                    if !Arc::ptr_eq(v_curr, v_new) {
                                        for val_item in v_new { if !v_curr.contains(val_item) { v_curr.push(val_item.clone()); } }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if let Some(state) = merged_state { output.push((state, 1)); }
            });
            feedback.leave_into_input(&templates_state_var);

            templates_state_var.as_collection().map(|(_id, state)| {
                let duration_val = if state.last_updated_timestamp > state.first_seen_timestamp {
                    // This timestamp math is simplified for u64 or similar direct subtractable types
                    state.last_updated_timestamp.clone().inner - state.first_seen_timestamp.clone().inner
                } else { 0 };
                let duration_secs = if duration_val == 0 { 1.0 } else { duration_val as f64 };
                let calculated_rate = if state.count > 0 { state.count as f64 / duration_secs } else { 0.0 };
                LogGroupDD {
                    template_id: state.template.id,
                    template_str: format_template_tokens(&state.template.tokens),
                    internal_template_tokens: state.template.tokens.clone(),
                    count: state.count,
                    first_seen_timestamp: state.first_seen_timestamp.clone(),
                    last_updated_timestamp: state.last_updated_timestamp.clone(),
                    rate: calculated_rate,
                    rate_of_change: 0.0,
                    aggregated_fields: state.aggregated_fields.clone(),
                }
            })
        });
        DifferentialDrain { raw_log_line_input, log_groups_collection: final_log_groups }
    }

    pub fn process_line(&mut self, line: String, time: TS) { // Confirmed TS
        self.raw_log_line_input.update_at((line, time.clone()), time, 1);
    }

    pub fn flush(&mut self, time: TS) { // Confirmed TS
        self.raw_log_line_input.advance_to(time);
    }
}

#[cfg(test)]
mod tests {
    type LogId = u64; // Define LogId for the test module as it was missing
    use super::*;
    use timely::communication::allocator::Thread;
    use timely::worker::Worker; 
    use differential_dataflow::operators::probe::Handle as ProbeHandle;
    use std::collections::{BTreeMap, HashSet}; 
    use timely::progress::timestamp::RootTimestamp; 

    fn run_drain_scenario(
        test_fn: impl FnOnce(
            &mut Worker<Thread>, 
            &mut DifferentialDrain<Thread, RootTimestamp>, 
            &mut ProbeHandle<RootTimestamp, LogGroupDD<RootTimestamp>>, 
            &mut ProbeHandle<RootTimestamp, (LogId, Uuid)>      
        ) + Send + 'static
    ) {
        timely::execute::execute_from_args(std::env::args(), move |worker| {
            let mut log_groups_probe = ProbeHandle::new();
            let mut assignments_probe = ProbeHandle::new(); 
            
            worker.dataflow(|scope| { // scope here is Child<'worker, Worker<Thread>, RootTimestamp>
                let mut drain = DifferentialDrain::new(scope); // new expects &mut Child<'a, Worker<A>, TS>
                                                              // Here A=Thread, TS=RootTimestamp, which matches.
                drain.log_groups_collection.probe_with(&mut log_groups_probe);
                test_fn(worker, &mut drain, &mut log_groups_probe, &mut assignments_probe);
            });
        }).expect("Timely dataflow computation failed.");
    }

    // Helper to collect the final state of LogGroupDDs from a probe.
    fn get_final_log_groups_map(probe: &mut ProbeHandle<RootTimestamp, LogGroupDD<RootTimestamp>>) -> HashMap<Uuid, LogGroupDD<RootTimestamp>> {
        let mut groups_map = HashMap::new();
        probe.with_read(|data| {
            for (_time, data_in_batch) in data.iter() {
                for (log_group, _event_ts, diff) in data_in_batch.iter() {
                    if *diff > 0 {
                        groups_map.insert(log_group.template_id, log_group.clone());
                    } else {
                        // If the exact log_group state was previously inserted and now retracted.
                        if groups_map.get(&log_group.template_id).map_or(false, |g| g == log_group) {
                             groups_map.remove(&log_group.template_id);
                        }
                    }
                }
            }
        });
        groups_map.retain(|_, g| g.count > 0); 
        groups_map
    }

    #[test]
    fn test_identical_log_lines_clustering() {
        run_drain_scenario(|worker, drain, lg_probe, _assign_probe| { 
            let ts_start = RootTimestamp::new(1);
            drain.process_line("Test message 1".to_string(), ts_start.clone());
            drain.process_line("Test message 1".to_string(), RootTimestamp::new(ts_start.inner + 1));
            drain.process_line("Test message 1".to_string(), RootTimestamp::new(ts_start.inner + 2));
            
            let final_ts = RootTimestamp::new(ts_start.inner + 3);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));

            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 1, "Should only find one group for identical messages");
            
            let group = groups_map.values().next().expect("Should have one group");
            assert_eq!(group.count, 3);
            assert_eq!(group.template_str, "Test message 1");
            assert_eq!(group.first_seen_timestamp, ts_start);
            assert_eq!(group.last_updated_timestamp, RootTimestamp::new(ts_start.inner + 2));
        });
    }
    
    // Other existing tests need to be updated to use RootTimestamp consistently
    // and potentially the new helper get_final_log_groups_map for clarity in assertions.
    // Also, if they use assign_probe, its argument should be passed.

    #[test]
    fn test_parameterized_log_lines_clustering() {
        run_drain_scenario(|worker, drain, lg_probe, _assign_probe| {
            let ts_start = RootTimestamp::new(10);
            drain.process_line("User 123 logged in".to_string(), ts_start.clone());
            drain.process_line("User 456 logged in".to_string(), RootTimestamp::new(ts_start.inner + 1));
            
            let final_ts = RootTimestamp::new(ts_start.inner + 2);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));

            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 1, "Should group parameterized lines together");
            if let Some(group) = groups_map.values().next() {
                assert_eq!(group.count, 2);
                assert_eq!(group.template_str, "User <*> logged in");
            } else {
                panic!("No group found for parameterized lines test.");
            }
        });
    }

    #[test]
    fn test_different_log_lines_separate_groups() {
         run_drain_scenario(|worker, drain, lg_probe, _assign_probe| {
            let ts1 = RootTimestamp::new(20);
            let ts2 = RootTimestamp::new(21);
            drain.process_line("First distinct message".to_string(), ts1.clone());
            drain.process_line("Second distinct message".to_string(), ts2.clone());
            
            let final_ts = RootTimestamp::new(22);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));

            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 2, "Should find two distinct groups.");
            
            let templates_found: HashSet<String> = groups_map.values().map(|g| g.template_str.clone()).collect();
            assert!(templates_found.contains("First distinct message"));
            assert!(templates_found.contains("Second distinct message"));
        });
    }
    
    #[test]
    fn test_numeric_parameter_aggregation() {
        run_drain_scenario(|worker, drain, lg_probe, _assign_probe| {
            let ts1 = RootTimestamp::new(30);
            let ts2 = RootTimestamp::new(31);
            drain.process_line("Request processed in 100 ms".to_string(), ts1.clone());
            drain.process_line("Request processed in 250 ms".to_string(), ts2.clone());

            let final_ts = RootTimestamp::new(32);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));
            
            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 1);
            if let Some(group) = groups_map.values().next() {
                assert_eq!(group.count, 2);
                assert_eq!(group.template_str, "Request processed in <*> ms");
                let agg_field = group.aggregated_fields.get("param_3"); 
                assert!(agg_field.is_some(), "Aggregated field for param_3 should exist. Fields: {:?}", group.aggregated_fields.keys());
                if let Some(FieldAggregation::Sum(s)) = agg_field {
                    assert_eq!(*s, 100 + 250);
                } else {
                    panic!("Expected Sum aggregation, found {:?}", agg_field);
                }
            } else {
                panic!("No group found for numeric aggregation test.");
            }
        });
    }
    
    #[test]
    fn test_string_parameter_aggregation() {
        run_drain_scenario(|worker, drain, lg_probe, _assign_probe| {
            let ts1 = RootTimestamp::new(40);
            let ts2 = RootTimestamp::new(41);
            let ts3 = RootTimestamp::new(42);
            drain.process_line("Login failed for user alice".to_string(), ts1.clone());
            drain.process_line("Login failed for user bob".to_string(), ts2.clone());
            drain.process_line("Login failed for user alice".to_string(), ts3.clone());

            let final_ts = RootTimestamp::new(43);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));

            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 1);
            if let Some(group) = groups_map.values().next() {
                assert_eq!(group.count, 3);
                assert_eq!(group.template_str, "Login failed for user <*>");
                let agg_field = group.aggregated_fields.get("param_4");
                assert!(agg_field.is_some(), "Aggregated field for param_4 should exist. Fields: {:?}", group.aggregated_fields.keys());
                if let Some(FieldAggregation::DistinctValues(vals)) = agg_field {
                    let distinct_users: HashSet<String> = vals.iter().cloned().collect();
                    assert_eq!(distinct_users.len(), 2, "Should have 2 distinct users");
                    assert!(distinct_users.contains("alice"));
                    assert!(distinct_users.contains("bob"));
                } else {
                    panic!("Expected DistinctValues aggregation, found {:?}", agg_field);
                }
            } else {
                 panic!("No group found for string aggregation test.");
            }
        });
    }

    #[test]
    fn test_basic_rate_calculation() {
        run_drain_scenario(|worker, drain, lg_probe, _assign_probe| {
            let ts_start = RootTimestamp::new(50);
            drain.process_line("Rate test message".to_string(), ts_start.clone());
            drain.process_line("Rate test message".to_string(), RootTimestamp::new(ts_start.inner + 1));
            drain.process_line("Rate test message".to_string(), RootTimestamp::new(ts_start.inner + 2));

            let final_ts = RootTimestamp::new(ts_start.inner + 3);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));
            
            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 1);
            if let Some(group) = groups_map.values().next() {
                assert_eq!(group.count, 3);
                let expected_duration = (RootTimestamp::new(ts_start.inner + 2).inner - ts_start.inner) as f64;
                let expected_rate = 3.0 / expected_duration; // 3 events / 2 "seconds"
                assert!((group.rate - expected_rate).abs() < 0.001, "Rate calculation is incorrect. Expected {}, got {}. Duration: {}", expected_rate, group.rate, expected_duration);
            } else {
                 panic!("No group found for rate calculation test.");
            }
        });
    }

     #[test]
    fn test_rate_calc_single_event() {
        run_drain_scenario(|worker, drain, lg_probe, _assign_probe| {
            let ts = RootTimestamp::new(60);
            drain.process_line("Single event rate test".to_string(), ts.clone());

            let final_ts = RootTimestamp::new(ts.inner + 1);
            drain.flush(final_ts.clone());
            worker.step_while(|| lg_probe.less_than(&final_ts));

            let groups_map = get_final_log_groups_map(lg_probe);
            assert_eq!(groups_map.len(), 1);
            if let Some(group) = groups_map.values().next() {
                assert_eq!(group.count, 1);
                // Duration is 1.0 if first_seen == last_updated
                assert!((group.rate - 1.0).abs() < 0.001, "Rate for single event incorrect. Expected 1.0, got {}", group.rate);
            } else {
                 panic!("No group found for single event rate test.");
            }
        });
    }
}
