// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

//! # Log Querying System
//!
//! This module provides structures and functions for querying log data.
//! The central component is the [`LogStore`], which acts as an abstraction layer
//! over a log data source, represented by an implementation of the [`Drain`](crate::drains::api::Drain) trait.
//!
//! It also defines LogQL (Log Query Language) related structures like [`LogQlQuery`],
//! [`StreamSelector`], and [`LineFilter`] to enable structured querying of logs.

use crate::drains::api::Drain;
use crate::log_group::LogGroup;
use crate::record::Record;
use chrono::{DateTime, Utc};
// Removed HashMap import as log_groups field is removed
use uuid::Uuid; // Added for function return type and usage

/// A generic log data store that interacts with a log source via the [`Drain`] trait.
///
/// `LogStore` provides an interface to query log groups and records. It is generic
/// over `D: Drain`, meaning it can work with any concrete drain implementation
/// that provides log data (e.g., in-memory drains, file-based drains).
///
/// Log groups are fetched dynamically from the underlying drain when queried.
#[derive(Debug, Clone)]
pub struct LogStore<D: Drain> {
    drain: D,
}

impl<D: Drain> LogStore<D> {
    /// Creates a new `LogStore` with the given drain.
    ///
    /// # Arguments
    ///
    /// * `drain` - An instance of a type implementing the `Drain` trait, which will
    ///   serve as the source of log data for this store.
    ///
    /// # Returns
    ///
    /// A new `LogStore` instance.
    pub fn new(drain: D) -> Self {
        Self { drain }
    }

    /// Retrieves a specific `LogGroup` by its ID.
    ///
    /// This method queries the underlying drain for all its log groups and then
    /// searches for the one with the matching ID.
    ///
    /// # Arguments
    ///
    /// * `id` - The `Uuid` of the `LogGroup` to retrieve.
    ///
    /// # Returns
    ///
    /// An `Option<LogGroup>` containing the found log group, or `None` if no
    /// group with the specified ID exists in the drain. The `LogGroup` is returned by value.
    pub fn get_log_group_by_id(&self, id: Uuid) -> Option<LogGroup> {
        self.drain
            .collect_log_groups()
            .into_iter()
            .find(|lg| lg.id == id)
    }

    /// Retrieves all `LogGroup`s that fall within a specified time range.
    ///
    /// This method queries the underlying drain for all its log groups and then
    /// filters them based on their timestamp.
    ///
    /// # Arguments
    ///
    /// * `start_time` - The `DateTime<Utc>` marking the beginning of the time range (inclusive).
    /// * `end_time` - The `DateTime<Utc>` marking the end of the time range (inclusive).
    ///
    /// # Returns
    ///
    /// A `Vec<LogGroup>` containing all log groups whose timestamp is within the
    /// specified range. The `LogGroup`s are returned by value.
    pub fn get_log_groups_in_range(
        &self,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Vec<LogGroup> {
        self.drain
            .collect_log_groups()
            .into_iter()
            .filter(|log_group| {
                let timestamp = log_group.get_time();
                timestamp >= start_time && timestamp <= end_time
            })
            .collect()
    }
}

// Define LogQL structures
/// Represents a selector for log streams in LogQL queries.
#[derive(Debug, Clone)]
pub enum StreamSelector {
    /// Selects log groups by a list of their unique identifiers.
    LogGroupIds(Vec<Uuid>),
}

/// Represents a line filter for LogQL queries.
#[derive(Debug, Clone)]
pub struct LineFilter {
    /// The substring that log lines must contain to match the filter.
    pub contains: String,
}

/// Represents a LogQL query, combining a stream selector and an optional line filter.
#[derive(Debug, Clone)]
pub struct LogQlQuery {
    /// The mechanism for selecting log streams (e.g., by group IDs).
    pub selector: StreamSelector,
    /// An optional filter to apply to the content of log lines.
    pub filter: Option<LineFilter>,
}

/// Specifies the source of records for a query, either by a direct ID or a LogQL query.
#[derive(Debug, Clone)]
pub enum QuerySource {
    /// Specifies a single log group by its unique identifier.
    ById(Uuid),
    /// Specifies records to be fetched using a full LogQL query.
    ByLogQl(LogQlQuery),
}

/// Executes a LogQL query against the provided `LogStore`.
///
/// This function processes a [`LogQlQuery`], retrieving records from the specified
/// log groups and applying any defined filters. It is generic over `D: Drain`
/// because `LogStore` is generic.
///
/// Due to lifetime considerations with the `Drain` trait (which returns owned `LogGroup`s),
/// this function returns a `Vec<Record>` (i.e., cloned, owned records) rather than references.
///
/// # Arguments
///
/// * `log_store` - A reference to the `LogStore` instance to query.
/// * `query` - A reference to the `LogQlQuery` defining the selection and filtering criteria.
///
/// # Returns
///
/// A `Vec<Record>` containing all records that match the query criteria.
pub fn execute_logql_query<D: Drain>(log_store: &LogStore<D>, query: &LogQlQuery) -> Vec<Record> {
    let mut records_batch: Vec<Record> = Vec::new();

    let collected_groups = log_store.drain.collect_log_groups();

    match &query.selector {
        StreamSelector::LogGroupIds(group_ids) => {
            for group_id in group_ids {
                if let Some(log_group) = collected_groups.iter().find(|lg| lg.id == *group_id) {
                    records_batch.push(log_group.base_record().clone()); // Clone base record
                    records_batch.extend(log_group.examples().iter().cloned()); // Clone example records
                }
            }
        }
    }

    if let Some(filter) = &query.filter {
        records_batch.retain(|record| record.to_string().contains(&filter.contains));
    }

    records_batch
}

/// Aggregates log records based on a query source and a time range.
///
/// This function retrieves records either by a specific log group ID or by a LogQL query,
/// and then filters these records to include only those within the specified time range.
/// It is generic over `D: Drain` due to its use of `LogStore`.
///
/// Similar to `execute_logql_query`, this function returns `Vec<Record>` (cloned records)
/// to manage lifetimes correctly with data sourced from the `Drain`.
///
/// # Arguments
///
/// * `log_store` - A reference to the `LogStore` instance.
/// * `query_source` - A [`QuerySource`] enum indicating whether to fetch records by ID or by a LogQL query.
/// * `start_time` - The `DateTime<Utc>` start of the aggregation range.
/// * `end_time` - The `DateTime<Utc>` end of the aggregation range.
///
/// # Returns
///
/// A `Vec<Record>` containing all records that match the query source and fall within the time range.
pub fn query_log_range_aggregation<D: Drain>(
    log_store: &LogStore<D>,
    query_source: QuerySource,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Vec<Record> {
    let initial_records: Vec<Record> = match query_source {
        QuerySource::ById(group_id) => {
            log_store.get_log_group_by_id(group_id).map_or_else(
                Vec::new, // If group not found, return empty vec
                |log_group| {
                    // If group found, collect its records (cloned)
                    let mut records = vec![log_group.base_record().clone()];
                    records.extend(log_group.examples().iter().cloned());
                    records
                },
            )
        }
        QuerySource::ByLogQl(logql_query) => {
            execute_logql_query(log_store, &logql_query) // Already returns Vec<Record>
        }
    };

    initial_records
        .into_iter()
        .filter(|record| {
            // record is now Record, not &Record
            record.uid.get_timestamp().is_some_and(|ts| {
                let (secs_u64, nanos) = ts.to_unix();
                let secs_i64 = secs_u64 as i64;
                if let Some(timestamp) = DateTime::from_timestamp(secs_i64, nanos) {
                    timestamp >= start_time && timestamp <= end_time
                } else {
                    false
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use chrono::Utc; // Keep Utc, add TimeZone for later proptest use
    use proptest::collection::vec as prop_vec; // Added for proptest
    use proptest::prelude::*; // Added for proptest
    use proptest::sample::subsequence; // Added for subsequence
    use std::collections::{HashMap, HashSet}; // Added for proptest, HashMap for mock drain
    use std::thread::sleep;
    use std::time::Duration as StdDuration;
    use uuid::{Uuid, Version}; // Added for proptest
                               // Removed unused import: use crate::drains::simple::SingleLayer;

    // Helper to create a record and get its timestamp
    fn get_record_timestamp(record: &Record) -> Option<DateTime<Utc>> {
        record.uid.get_timestamp().and_then(|ts| {
            let (secs_u64, nanos) = ts.to_unix();
            let secs_i64 = secs_u64 as i64;
            DateTime::from_timestamp(secs_i64, nanos)
        })
    }

    // --- Proptest Strategies ---

    // Strategy for Record content
    fn arb_record_content() -> impl Strategy<Value = String> {
        prop_vec(r"[a-zA-Z0-9]+", 1..10usize) // Generate 1 to 9 words
            .prop_map(|words| words.join(" "))
    }

    // Strategy for Record
    fn arb_record() -> impl Strategy<Value = Record> {
        arb_record_content().prop_map(Record::new)
    }

    // Strategy for LineFilter
    fn arb_line_filter() -> impl Strategy<Value = LineFilter> {
        "[a-zA-Z0-9]{1,10}".prop_map(|s| LineFilter { contains: s })
    }

    // Strategy for Option<LineFilter>
    fn arb_optional_line_filter() -> impl Strategy<Value = Option<LineFilter>> {
        prop_oneof![Just(None), arb_line_filter().prop_map(Some),]
    }

    // Strategy for DateTime<Utc>
    // Note: ChronoDuration is already aliased in the existing test module
    fn arb_datetime_utc() -> impl Strategy<Value = DateTime<Utc>> {
        (0i64..3600 * 24 * 30) // Offset in seconds (e.g., up to 30 days in the past from now)
            .prop_map(|offset_secs| Utc::now() - ChronoDuration::seconds(offset_secs))
    }

    // Strategy for LogGroup
    fn arb_log_group() -> impl Strategy<Value = LogGroup> {
        (arb_record(), prop_vec(arb_record(), 0..5usize)).prop_map(
            |(base_record, example_records)| {
                let mut group = LogGroup::new(base_record);
                for rec in example_records {
                    group.add_example(rec);
                }
                group
            },
        )
    }

    // This will be used to construct LogStore and a list of its valid Uuids for selectors
    fn arb_log_store_data() -> impl Strategy<Value = (Vec<LogGroup>, Vec<Uuid>)> {
        prop_vec(arb_log_group(), 0..10usize) // 0 to 10 log groups
            .prop_map(|groups| {
                let group_ids = groups.iter().map(|g| g.id).collect::<Vec<Uuid>>();
                (groups, group_ids)
            })
    }

    // Strategy for StreamSelector
    fn arb_stream_selector(valid_ids: Vec<Uuid>) -> impl Strategy<Value = StreamSelector> {
        if valid_ids.is_empty() {
            return Just(StreamSelector::LogGroupIds(Vec::new())).boxed();
        }
        // Strategy to pick a subsequence of valid_ids
        let id_subset_strategy = subsequence(valid_ids.clone(), 0..valid_ids.len())
            .prop_map(StreamSelector::LogGroupIds);

        // Strategy to generate some random Uuids (potentially not in the store)
        let random_ids_strategy = prop_vec(Just(Uuid::new_v4()), 0..3usize) // 0 to 3 random UUIDs
            .prop_map(StreamSelector::LogGroupIds);

        prop_oneof![
            id_subset_strategy,
            random_ids_strategy,
            // Mix of valid and random
            (
                subsequence(valid_ids.clone(), 0..valid_ids.len()),
                prop_vec(Just(Uuid::new_v4()), 0..2usize)
            )
                .prop_map(|(mut subset, mut random_guids)| {
                    subset.append(&mut random_guids);
                    StreamSelector::LogGroupIds(subset)
                })
        ]
        .boxed()
    }

    // Strategy for LogQlQuery
    fn arb_logql_query(valid_ids: Vec<Uuid>) -> impl Strategy<Value = LogQlQuery> {
        (arb_stream_selector(valid_ids), arb_optional_line_filter())
            .prop_map(|(selector, filter)| LogQlQuery { selector, filter })
    }

    // Combined strategy for log groups and a query based on those groups
    fn arb_log_groups_and_query() -> impl Strategy<Value = (Vec<LogGroup>, LogQlQuery)> {
        arb_log_store_data()
            .prop_flat_map(|(groups, group_ids)| (Just(groups), arb_logql_query(group_ids)))
    }

    // Combined strategy for log groups, query, and time range
    fn arb_log_groups_query_and_times(
    ) -> impl Strategy<Value = (Vec<LogGroup>, LogQlQuery, DateTime<Utc>, DateTime<Utc>)> {
        arb_log_store_data().prop_flat_map(|(groups, group_ids)| {
            (
                Just(groups),
                arb_logql_query(group_ids),
                arb_datetime_utc(),
                arb_datetime_utc(),
            )
        })
    }

    #[test]
    fn test_record_uuid_timestamp_extraction() {
        let record = Record::new("Test record for UUID and timestamp".to_string());
        assert_eq!(record.uid.get_version(), Some(Version::Mac)); // v1 UUID

        let timestamp = get_record_timestamp(&record);
        assert!(
            timestamp.is_some(),
            "Timestamp should be extractable from record UID"
        );
    }

    #[test]
    fn test_log_group_timestamp_extraction() {
        let now = Utc::now();
        sleep(StdDuration::from_millis(10)); // Ensure time moves forward a bit
        let record = Record::new("Test record for LogGroup timestamp".to_string());
        let log_group = LogGroup::new(record);
        sleep(StdDuration::from_millis(10)); // Ensure time moves forward a bit
        let after_creation = Utc::now();

        let group_time = log_group.get_time();

        // Looser check: group_time should be between when we started and finished creation.
        // More precise check might require controlling Uuid::new_v1 which is complex.
        assert!(
            group_time > now && group_time < after_creation,
            "LogGroup time {:?} should be between {:?} and {:?}",
            group_time,
            now,
            after_creation
        );
        // Also check it's reasonably close to now (e.g., within a few seconds)
        assert!(
            (Utc::now() - group_time).num_seconds() < 5,
            "LogGroup time should be recent"
        );
    }

    // Mock Drain for testing LogStore
    #[derive(Clone)] // Added Clone
    struct MockDrain {
        groups: HashMap<Uuid, LogGroup>,
    }

    impl MockDrain {
        fn new() -> Self {
            Self {
                groups: HashMap::new(),
            }
        }

        #[allow(dead_code)] // This method is used in tests that might be temporarily commented out
        fn add_group(&mut self, group: LogGroup) {
            self.groups.insert(group.id, group);
        }
    }

    impl Drain for MockDrain {
        fn process_line(&mut self, _line: String) -> anyhow::Result<bool> {
            // Not used in these LogStore tests
            Ok(false)
        }

        fn collect_log_groups(&self) -> Vec<LogGroup> {
            self.groups.values().cloned().collect()
        }
    }

    #[test]
    fn test_log_store_new() {
        let mock_drain = MockDrain::new();
        let store = LogStore::new(mock_drain);
        // Basic check: new store with an empty drain should yield no groups.
        // Further checks depend on how LogStore interacts with Drain,
        // e.g., if it immediately collects groups or does so on demand.
        // For now, just ensuring it can be created.
        assert!(
            store
                .get_log_groups_in_range(Utc::now(), Utc::now())
                .is_empty(),
            "New LogStore with empty drain should have no groups in range"
        );
    }

    #[test]
    fn test_log_store_get_by_id() {
        let record1 = Record::new("Record 1 for get_by_id".to_string());
        let group1 = LogGroup::new(record1);
        let id1 = group1.id;

        let mut mock_drain = MockDrain::new();
        mock_drain.add_group(group1.clone()); // Clone group1 as it's used later
        let store = LogStore::new(mock_drain);

        let found_group = store.get_log_group_by_id(id1);
        assert!(found_group.is_some(), "Should find existing group by ID");
        assert_eq!(found_group.unwrap().id, id1);

        let non_existent_id = Uuid::new_v4();
        let not_found_group = store.get_log_group_by_id(non_existent_id);
        assert!(
            not_found_group.is_none(),
            "Should return None for non-existent group ID"
        );
    }

    #[test]
    fn test_log_store_get_in_range() {
        let mut mock_drain = MockDrain::new();
        let base_time = Utc::now();

        let r1 = Record::new("g1".to_string());
        let g1 = LogGroup::new(r1);
        let time1 = g1.get_time();
        mock_drain.add_group(g1.clone());
        sleep(StdDuration::from_millis(100)); // Ensure timestamps are distinct

        let r2 = Record::new("g2".to_string());
        let g2 = LogGroup::new(r2);
        let time2 = g2.get_time();
        mock_drain.add_group(g2.clone());
        sleep(StdDuration::from_millis(100));

        let r3 = Record::new("g3".to_string());
        let g3 = LogGroup::new(r3);
        let time3 = g3.get_time();
        mock_drain.add_group(g3.clone());

        let store = LogStore::new(mock_drain);

        assert!(time1 < time2, "time1 should be less than time2");
        assert!(time2 < time3, "time2 should be less than time3");

        let all_groups = store.get_log_groups_in_range(time1, time3);
        assert_eq!(
            all_groups.len(),
            3,
            "Should find all 3 groups. Found: {:?}",
            all_groups.iter().map(|g| g.id).collect::<Vec<_>>()
        );

        let some_groups_middle = store.get_log_groups_in_range(
            time1 + ChronoDuration::milliseconds(50),
            time3 - ChronoDuration::milliseconds(50),
        );
        assert_eq!(some_groups_middle.len(), 1, "Should find 1 group (g2)");
        assert_eq!(some_groups_middle[0].id, g2.id);

        let some_groups_first_two = store.get_log_groups_in_range(time1, time2);
        assert_eq!(
            some_groups_first_two.len(),
            2,
            "Should find 2 groups (g1, g2)"
        );

        let no_groups_before = store.get_log_groups_in_range(
            base_time - ChronoDuration::seconds(10),
            base_time - ChronoDuration::seconds(5),
        );
        assert_eq!(
            no_groups_before.len(),
            0,
            "Should find no groups (range before all)"
        );

        let no_groups_after = store.get_log_groups_in_range(
            time3 + ChronoDuration::seconds(5),
            time3 + ChronoDuration::seconds(10),
        );
        assert_eq!(
            no_groups_after.len(),
            0,
            "Should find no groups (range after all)"
        );

        let exact_match_g2 = store.get_log_groups_in_range(time2, time2);
        assert_eq!(
            exact_match_g2.len(),
            1,
            "Should find g2 with exact time match"
        );
        assert_eq!(exact_match_g2[0].id, g2.id);

        let edge_start = store.get_log_groups_in_range(time1, time1);
        assert_eq!(
            edge_start.len(),
            1,
            "Should find g1 when it's exactly on start_time"
        );
        assert_eq!(edge_start[0].id, g1.id);

        let edge_end = store.get_log_groups_in_range(time3, time3);
        assert_eq!(
            edge_end.len(),
            1,
            "Should find g3 when it's exactly on end_time"
        );
        assert_eq!(edge_end[0].id, g3.id);
    }

    #[test]
    fn test_query_log_range_aggregation() {
        let mut mock_drain = MockDrain::new();
        let record_template = Record::new("Log group for aggregation test".to_string());
        let mut log_group = LogGroup::new(record_template);
        let group_id = log_group.id;
        let base_time = Utc::now();

        let ex_r1 = Record::new("Example 1".to_string());
        let time1 = get_record_timestamp(&ex_r1).unwrap();
        log_group.add_example(ex_r1);
        sleep(StdDuration::from_millis(50));

        let ex_r2 = Record::new("Example 2".to_string());
        let time2 = get_record_timestamp(&ex_r2).unwrap();
        log_group.add_example(ex_r2);
        sleep(StdDuration::from_millis(50));

        let ex_r3 = Record::new("Example 3".to_string());
        let time3 = get_record_timestamp(&ex_r3).unwrap();
        log_group.add_example(ex_r3);

        mock_drain.add_group(log_group);
        let store = LogStore::new(mock_drain);

        let non_existent_id = Uuid::new_v4();
        let results_not_found = query_log_range_aggregation(
            &store, // LogStore<MockDrain>
            QuerySource::ById(non_existent_id),
            base_time,  // DateTime<Utc>
            Utc::now(), // DateTime<Utc>
        );
        assert!(
            results_not_found.is_empty(),
            "Should return empty for non-existent group ID"
        );

        let results_none_in_range = query_log_range_aggregation(
            &store,
            QuerySource::ById(group_id),
            base_time - ChronoDuration::seconds(10),
            base_time - ChronoDuration::seconds(5),
        );
        assert!(
            results_none_in_range.is_empty(),
            "Should return empty if no records in time range"
        );

        let results_some_in_range = query_log_range_aggregation(
            &store,
            QuerySource::ById(group_id),
            time1 + ChronoDuration::milliseconds(25), // start_time
            time3 - ChronoDuration::milliseconds(25), // end_time
        );
        assert_eq!(
            results_some_in_range.len(),
            1,
            "Should find 1 record in the middle of the range"
        );
        assert_eq!(
            get_record_timestamp(&results_some_in_range[0]).unwrap(), // Added &
            time2
        );

        let results_all_in_range =
            query_log_range_aggregation(&store, QuerySource::ById(group_id), time1, time3);
        assert_eq!(
            results_all_in_range.len(),
            3,
            "Should find all 3 records in the range"
        );

        let results_edge_start =
            query_log_range_aggregation(&store, QuerySource::ById(group_id), time1, time1);
        assert_eq!(
            results_edge_start.len(),
            1,
            "Should find record 1 when it is exactly on start_time"
        );
        assert_eq!(get_record_timestamp(&results_edge_start[0]).unwrap(), time1); // Added &

        let results_edge_end =
            query_log_range_aggregation(&store, QuerySource::ById(group_id), time3, time3);
        assert_eq!(
            results_edge_end.len(),
            1,
            "Should find record 3 when it is exactly on end_time"
        );
        assert_eq!(get_record_timestamp(&results_edge_end[0]).unwrap(), time3); // Added &

        let results_first_two =
            query_log_range_aggregation(&store, QuerySource::ById(group_id), time1, time2);
        assert_eq!(results_first_two.len(), 2, "Should find first two records");
    }

    // --- Property Tests ---
    // Helper function to create LogStore<MockDrain> from Vec<LogGroup>
    fn log_store_from_mock_groups(groups: Vec<LogGroup>) -> LogStore<MockDrain> {
        let mut drain = MockDrain::new();
        for group in groups {
            drain.add_group(group);
        }
        LogStore::new(drain)
    }

    proptest! {
        #[test]
        fn prop_execute_logql_query(
            (log_groups_vec, query) in arb_log_groups_and_query()
        ) {
            // Use the helper to create LogStore with MockDrain
            let log_store = log_store_from_mock_groups(log_groups_vec.clone());
            let results = execute_logql_query(&log_store, &query);

            let selected_group_ids_set: HashSet<Uuid> = match &query.selector {
                StreamSelector::LogGroupIds(ids) => ids.iter().cloned().collect(),
            };

            for record_in_result in &results {
                let mut record_belongs_to_a_selected_group = false;
                for group in &log_groups_vec { // Use log_groups_vec here
                    if selected_group_ids_set.contains(&group.id) && (group.base_record().uid == record_in_result.uid || group.examples().iter().any(|ex| ex.uid == record_in_result.uid)) {
                        record_belongs_to_a_selected_group = true;
                        break;
                    }
                }
                prop_assert!(record_belongs_to_a_selected_group, "Record {} from results (content: '{}') does not belong to any selected group. Selected groups: {:?}", record_in_result.uid, record_in_result.to_string(), selected_group_ids_set);

                if let Some(filter) = &query.filter {
                    prop_assert!(record_in_result.to_string().contains(&filter.contains),
                                 "Record {} from results (content: '{}') does not match filter '{}'", record_in_result.uid, record_in_result.to_string(), filter.contains);
                }
            }

            for group in &log_groups_vec { // Use log_groups_vec here
                if selected_group_ids_set.contains(&group.id) {
                    let records_to_check = group.examples().clone();
                    for original_record in records_to_check {
                        let matches_filter = query.filter.as_ref().is_none_or(|f| original_record.to_string().contains(&f.contains));
                        if matches_filter {
                            prop_assert!(results.iter().any(|res_rec| res_rec.uid == original_record.uid),
                                         "Original record {} (content: '{}') from group {} matches filter but not found in results. Filter: {:?}", original_record.uid, original_record.to_string(), group.id, query.filter.as_ref().map(|f| &f.contains));
                        } else {
                            prop_assert!(!results.iter().any(|res_rec| res_rec.uid == original_record.uid),
                                         "Original record {} (content: '{}') from group {} does NOT match filter but IS found in results. Filter: {:?}", original_record.uid, original_record.to_string(), group.id, query.filter.as_ref().map(|f| &f.contains));
                        }
                    }
                }
            }
        }
    }

    proptest! {
        #[test]
        fn prop_query_log_range_aggregation_with_logql(
            (log_groups_vec, query, time1, time2) in arb_log_groups_query_and_times()
        ) {
            let log_store = log_store_from_mock_groups(log_groups_vec.clone()); // Use helper
            let (start_time, end_time) = if time1 <= time2 { (time1, time2) } else { (time2, time1) };
            let results = query_log_range_aggregation(&log_store, QuerySource::ByLogQl(query.clone()), start_time, end_time);

            let selected_group_ids_set: HashSet<Uuid> = match &query.selector {
                StreamSelector::LogGroupIds(ids) => ids.iter().cloned().collect(),
            };

            for record_in_result in &results {
                let mut record_belongs_to_a_selected_group = false;
                for group in &log_groups_vec { // Use log_groups_vec here
                    if selected_group_ids_set.contains(&group.id) && (group.base_record().uid == record_in_result.uid || group.examples().iter().any(|ex| ex.uid == record_in_result.uid)) {
                        record_belongs_to_a_selected_group = true;
                        break;
                    }
                }
                prop_assert!(record_belongs_to_a_selected_group, "Record {} from results (content: '{}') does not belong to selected group. Selected: {:?}", record_in_result.uid, record_in_result.to_string(), selected_group_ids_set);

                if let Some(filter) = &query.filter {
                    prop_assert!(record_in_result.to_string().contains(&filter.contains),
                                 "Record {} (content: '{}') does not match filter '{}'", record_in_result.uid, record_in_result.to_string(), filter.contains);
                }

                let record_time = get_record_timestamp(record_in_result).expect("Record in results should have a timestamp");
                prop_assert!(record_time >= start_time && record_time <= end_time,
                             "Record time {:?} for {} (content: '{}') is outside range [{:?}, {:?}]", record_time, record_in_result.uid, record_in_result.to_string(), start_time, end_time);
            }

            for group in &log_groups_vec { // Use log_groups_vec here
                if selected_group_ids_set.contains(&group.id) {
                    let mut records_to_check = group.examples().clone();
                    records_to_check.push(group.base_record().clone());

                    for original_record in records_to_check {
                        let matches_filter = query.filter.as_ref().is_none_or(|f| original_record.to_string().contains(&f.contains));
                        let record_time_option = get_record_timestamp(&original_record);
                        prop_assert!(record_time_option.is_some(), "Original record {} (content: '{}') must have a timestamp for time range check.", original_record.uid, original_record.to_string());
                        let record_time = record_time_option.unwrap();
                        let matches_time = record_time >= start_time && record_time <= end_time;

                        if matches_filter && matches_time {
                            prop_assert!(results.iter().any(|res_rec| res_rec.uid == original_record.uid),
                                         "Original record {} (content: '{}', time: {:?}) from group {} matches filter and time but not found in results. Query: {:?}, Filter: {:?}, Time Range: [{:?}, {:?}]",
                                         original_record.uid, original_record.to_string(), record_time, group.id, query.selector, query.filter.as_ref().map(|f| &f.contains), start_time, end_time);
                        } else {
                             prop_assert!(!results.iter().any(|res_rec| res_rec.uid == original_record.uid),
                                         "Original record {} (content: '{}', time: {:?}) from group {} (matches_filter: {}, matches_time: {}) found in results but should not be. Query: {:?}, Filter: {:?}, Time Range: [{:?}, {:?}]",
                                         original_record.uid, original_record.to_string(), record_time, group.id, matches_filter, matches_time, query.selector, query.filter.as_ref().map(|f| &f.contains), start_time, end_time);
                        }
                    }
                }
            }
        }
    }
}
