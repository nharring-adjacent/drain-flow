// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::collections::HashMap;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::log_group::LogGroup;
use crate::record::Record; // Added for function return type and usage

pub struct LogStore {
    log_groups: HashMap<Uuid, LogGroup>,
}

impl LogStore {
    pub fn new() -> Self {
        Self {
            log_groups: HashMap::new(),
        }
    }

    pub fn add_log_group(&mut self, log_group: LogGroup) {
        self.log_groups.insert(log_group.id, log_group);
    }

    pub fn from_log_groups(groups: impl IntoIterator<Item = LogGroup>) -> Self {
        let mut store = Self::new();
        for group in groups {
            store.add_log_group(group);
        }
        store
    }

    pub fn get_log_group_by_id(&self, id: Uuid) -> Option<&LogGroup> {
        self.log_groups.get(&id)
    }

    pub fn get_log_groups_in_range(&self, start_time: DateTime<Utc>, end_time: DateTime<Utc>) -> Vec<&LogGroup> {
        self.log_groups
            .values()
            .filter(|log_group| {
                let timestamp = log_group.get_time();
                timestamp >= start_time && timestamp <= end_time
            })
            .collect()
    }
}

pub fn query_log_range_aggregation<'a>(
    log_store: &'a LogStore,
    group_id: Uuid,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
) -> Vec<&'a Record> {
    match log_store.get_log_group_by_id(group_id) {
        Some(log_group) => {
            log_group
                .get_examples()
                .into_iter()
                .filter(|record| {
                    record.uid.get_timestamp().map_or(false, |ts| {
                        let (secs_u64, nanos) = ts.to_unix();
                        // This conversion is safe for typical timestamp ranges
                        let secs_i64 = secs_u64 as i64; 
                        if let Some(timestamp) = DateTime::from_timestamp(secs_i64, nanos) {
                            timestamp >= start_time && timestamp <= end_time
                        } else {
                            false // Timestamp conversion failed
                        }
                    })
                })
                .collect()
        }
        None => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Utc}; // Removed Duration from here
    use uuid::{Uuid, Version};
    use std::thread::sleep;
    use std::time::Duration as StdDuration; // Added for sleep
    use chrono::Duration as ChronoDuration; // Added for chrono operations

    // Helper to create a record and get its timestamp
    fn get_record_timestamp(record: &Record) -> Option<DateTime<Utc>> {
        record.uid.get_timestamp().and_then(|ts| {
            let (secs_u64, nanos) = ts.to_unix();
            let secs_i64 = secs_u64 as i64;
            DateTime::from_timestamp(secs_i64, nanos)
        })
    }

    #[test]
    fn test_record_uuid_timestamp_extraction() {
        let record = Record::new("Test record for UUID and timestamp".to_string());
        assert_eq!(record.uid.get_version(), Some(Version::Mac)); // v1 UUID

        let timestamp = get_record_timestamp(&record);
        assert!(timestamp.is_some(), "Timestamp should be extractable from record UID");
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
        assert!(group_time > now && group_time < after_creation, "LogGroup time {:?} should be between {:?} and {:?}", group_time, now, after_creation);
        // Also check it's reasonably close to now (e.g., within a few seconds)
        assert!((Utc::now() - group_time).num_seconds() < 5, "LogGroup time should be recent");
    }

    #[test]
    fn test_log_store_new_and_add() {
        let mut store = LogStore::new();
        assert_eq!(store.log_groups.len(), 0, "New LogStore should be empty");

        let record = Record::new("Test record for new_and_add".to_string());
        let log_group = LogGroup::new(record);
        let group_id = log_group.id;

        store.add_log_group(log_group);
        assert_eq!(store.log_groups.len(), 1, "LogStore should have one group after adding");

        let retrieved_group = store.get_log_group_by_id(group_id);
        assert!(retrieved_group.is_some(), "Should retrieve the added log group");
        assert_eq!(retrieved_group.unwrap().id, group_id, "Retrieved group ID should match");
    }

    #[test]
    fn test_log_store_from_log_groups() {
        let record1 = Record::new("Record 1 for from_log_groups".to_string());
        let group1 = LogGroup::new(record1);
        let id1 = group1.id;

        sleep(StdDuration::from_millis(10));
        let record2 = Record::new("Record 2 for from_log_groups".to_string());
        let group2 = LogGroup::new(record2);
        let id2 = group2.id;

        let groups = vec![group1, group2]; // group1 and group2 are moved here
        let store = LogStore::from_log_groups(groups);

        assert_eq!(store.log_groups.len(), 2, "LogStore should contain two groups");
        assert!(store.get_log_group_by_id(id1).is_some(), "Group 1 should be in the store");
        assert!(store.get_log_group_by_id(id2).is_some(), "Group 2 should be in the store");
    }

    #[test]
    fn test_log_store_get_by_id() {
        let record1 = Record::new("Record 1 for get_by_id".to_string());
        let group1 = LogGroup::new(record1);
        let id1 = group1.id;

        let store = LogStore::from_log_groups(vec![group1]);

        let found_group = store.get_log_group_by_id(id1);
        assert!(found_group.is_some(), "Should find existing group by ID");
        assert_eq!(found_group.unwrap().id, id1);

        let non_existent_id = Uuid::new_v4(); // Different type of UUID, but fine for testing non-existence
        let not_found_group = store.get_log_group_by_id(non_existent_id);
        assert!(not_found_group.is_none(), "Should return None for non-existent group ID");
    }

    #[test]
    fn test_log_store_get_in_range() {
        let mut groups_to_add = Vec::new();
        let base_time = Utc::now();

        // Create groups with timestamps approximately 100ms apart
        let r1 = Record::new("g1".to_string()); let g1 = LogGroup::new(r1); // ~base_time
        let id1 = g1.id;
        groups_to_add.push(g1); 
        sleep(StdDuration::from_millis(100));

        let r2 = Record::new("g2".to_string()); let g2 = LogGroup::new(r2); // ~base_time + 100ms
        let id2 = g2.id;
        groups_to_add.push(g2);
        sleep(StdDuration::from_millis(100));

        let r3 = Record::new("g3".to_string()); let g3 = LogGroup::new(r3); // ~base_time + 200ms
        let id3 = g3.id;
        groups_to_add.push(g3);
        
        let store = LogStore::from_log_groups(groups_to_add);
        
        // Fetch the actual stored groups by their original IDs to get their correct timestamps
        let stored_g1 = store.get_log_group_by_id(id1).expect("g1 not found in store");
        let time1 = stored_g1.get_time();
        let stored_g2 = store.get_log_group_by_id(id2).expect("g2 not found in store");
        let time2 = stored_g2.get_time();
        let stored_g3 = store.get_log_group_by_id(id3).expect("g3 not found in store");
        let time3 = stored_g3.get_time();

        // Ensure times are ordered as expected due to sleep, with some tolerance
        assert!(time1 < time2, "time1 should be less than time2");
        assert!(time2 < time3, "time2 should be less than time3");


        // Scenario 1: Range includes all
        let all_groups = store.get_log_groups_in_range(time1, time3);
        assert_eq!(all_groups.len(), 3, "Should find all 3 groups. Found: {:?}", all_groups.iter().map(|g| g.id).collect::<Vec<_>>());


        // Scenario 2: Range includes some (middle one)
        let some_groups_middle = store.get_log_groups_in_range(time1 + ChronoDuration::milliseconds(50), time3 - ChronoDuration::milliseconds(50));
        assert_eq!(some_groups_middle.len(), 1, "Should find 1 group (g2)");
        assert_eq!(some_groups_middle[0].id, stored_g2.id);
        
        // Scenario 3: Range includes some (first two)
        let some_groups_first_two = store.get_log_groups_in_range(time1, time2);
        assert_eq!(some_groups_first_two.len(), 2, "Should find 2 groups (g1, g2)");

        // Scenario 4: Range includes none (before all)
        let no_groups_before = store.get_log_groups_in_range(base_time - ChronoDuration::seconds(10), base_time - ChronoDuration::seconds(5));
        assert_eq!(no_groups_before.len(), 0, "Should find no groups (range before all)");

        // Scenario 5: Range includes none (after all)
        let no_groups_after = store.get_log_groups_in_range(time3 + ChronoDuration::seconds(5), time3 + ChronoDuration::seconds(10));
        assert_eq!(no_groups_after.len(), 0, "Should find no groups (range after all)");

        // Scenario 6: Range is start_time == end_time (exact match for g2)
        let exact_match_g2 = store.get_log_groups_in_range(time2, time2);
        assert_eq!(exact_match_g2.len(), 1, "Should find g2 with exact time match");
        assert_eq!(exact_match_g2[0].id, stored_g2.id);
        
        // Scenario 7: Edge case - g1 exactly on start_time
        let edge_start = store.get_log_groups_in_range(time1, time1);
        assert_eq!(edge_start.len(), 1, "Should find g1 when it's exactly on start_time");
        assert_eq!(edge_start[0].id, stored_g1.id);

        // Scenario 8: Edge case - g3 exactly on end_time
        let edge_end = store.get_log_groups_in_range(time3, time3);
         assert_eq!(edge_end.len(), 1, "Should find g3 when it's exactly on end_time");
        assert_eq!(edge_end[0].id, stored_g3.id);
    }
    
    #[test]
    fn test_query_log_range_aggregation() {
        let mut store = LogStore::new();
        let record_template = Record::new("Log group for aggregation test".to_string());
        let mut log_group = LogGroup::new(record_template);
        let group_id = log_group.id;
        let base_time = Utc::now();

        // Create example records with varying timestamps
        let ex_r1 = Record::new("Example 1".to_string()); // ~base_time
        let time1 = get_record_timestamp(&ex_r1).unwrap();
        log_group.add_example(ex_r1);
        sleep(StdDuration::from_millis(50));

        let ex_r2 = Record::new("Example 2".to_string()); // ~base_time + 50ms
        let time2 = get_record_timestamp(&ex_r2).unwrap();
        log_group.add_example(ex_r2);
        sleep(StdDuration::from_millis(50));
        
        let ex_r3 = Record::new("Example 3".to_string()); // ~base_time + 100ms
        let time3 = get_record_timestamp(&ex_r3).unwrap();
        log_group.add_example(ex_r3);

        store.add_log_group(log_group);
        
        // Scenario 1: Group ID not found
        let non_existent_id = Uuid::new_v4();
        let results_not_found = query_log_range_aggregation(&store, non_existent_id, base_time, Utc::now());
        assert!(results_not_found.is_empty(), "Should return empty for non-existent group ID");

        // Scenario 2: Group ID found, but no records in the time range
        let results_none_in_range = query_log_range_aggregation(&store, group_id, base_time - ChronoDuration::seconds(10), base_time - ChronoDuration::seconds(5));
        assert!(results_none_in_range.is_empty(), "Should return empty if no records in time range");

        // Scenario 3: Group ID found, some records in the time range (middle one)
        let results_some_in_range = query_log_range_aggregation(&store, group_id, time1 + ChronoDuration::milliseconds(25) , time3 - ChronoDuration::milliseconds(25));
        assert_eq!(results_some_in_range.len(), 1, "Should find 1 record in the middle of the range");
        assert_eq!(get_record_timestamp(results_some_in_range[0]).unwrap(), time2);


        // Scenario 4: Group ID found, all records in the time range
        let results_all_in_range = query_log_range_aggregation(&store, group_id, time1, time3);
        assert_eq!(results_all_in_range.len(), 3, "Should find all 3 records in the range");

        // Scenario 5: Edge cases for time ranges
        // Record 1 exactly on start_time
        let results_edge_start = query_log_range_aggregation(&store, group_id, time1, time1);
        assert_eq!(results_edge_start.len(), 1, "Should find record 1 when it is exactly on start_time");
        assert_eq!(get_record_timestamp(results_edge_start[0]).unwrap(), time1);

        // Record 3 exactly on end_time
        let results_edge_end = query_log_range_aggregation(&store, group_id, time3, time3);
        assert_eq!(results_edge_end.len(), 1, "Should find record 3 when it is exactly on end_time");
        assert_eq!(get_record_timestamp(results_edge_end[0]).unwrap(), time3);
        
        // Range that includes first two
        let results_first_two = query_log_range_aggregation(&store, group_id, time1, time2);
        assert_eq!(results_first_two.len(), 2, "Should find first two records");
    }
}
