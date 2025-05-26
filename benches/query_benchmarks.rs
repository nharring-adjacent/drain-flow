use criterion::{black_box, criterion_group, criterion_main, Criterion};
use log_ql_playground_project::{
    LogStore, Record, LogGroup, LogQlQuery, StreamSelector, LineFilter, QuerySource,
    query_log_range_aggregation, execute_logql_query
};
use uuid::Uuid;
use chrono::{Utc, Duration as ChronoDuration, DateTime};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::collections::VecDeque; // Included as per prompt, though not used in this specific helper

// Helper to create a LogStore with a specific number of groups and records
fn setup_log_store(num_groups: usize, records_per_group: usize, mut rng: &mut StdRng) -> (LogStore, Vec<Uuid>) {
    let mut log_groups = Vec::new();
    let mut group_ids = Vec::new();
    for i in 0..num_groups {
        // Create a unique base record content for each group
        let base_record_content = format!("Base record for group {} - {}", Uuid::from_u128(rng.gen()), i);
        let base_record = Record::new(base_record_content); 
        let mut group = LogGroup::new(base_record);
        group_ids.push(group.id);

        for j in 0..records_per_group {
            let example_content = format!("Example record {} for group {} - {}", j, i, Uuid::from_u128(rng.gen()));
            let record_str = if j % 2 == 0 {
                format!("{} common_term_for_filtering", example_content)
            } else {
                example_content
            };
            group.add_example(Record::new(record_str));
        }
        log_groups.push(group);
    }
    (LogStore::from_log_groups(log_groups), group_ids)
}

fn benchmark_execute_logql_query(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42); // Fixed seed for deterministic results
    let (log_store, group_ids) = setup_log_store(10, 100, &mut rng);
    
    let query = LogQlQuery {
        selector: StreamSelector::LogGroupIds(group_ids.iter().take(5).cloned().collect()), // Select first 5 groups
        filter: Some(LineFilter { contains: "common_term_for_filtering".to_string() }),
    };

    c.bench_function("execute_logql_query_10g_100rpg_5sel_filter", |b| {
        b.iter(|| execute_logql_query(black_box(&log_store), black_box(&query)))
    });

    let query_no_filter = LogQlQuery {
        selector: StreamSelector::LogGroupIds(group_ids.iter().take(5).cloned().collect()),
        filter: None,
    };
    c.bench_function("execute_logql_query_10g_100rpg_5sel_nofilter", |b| {
        b.iter(|| execute_logql_query(black_box(&log_store), black_box(&query_no_filter)))
    });
}

fn benchmark_qra_logql(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let (log_store, group_ids) = setup_log_store(10, 100, &mut rng);
    
    let query = LogQlQuery {
        selector: StreamSelector::LogGroupIds(group_ids.iter().take(3).cloned().collect()), // Select first 3 groups
        filter: Some(LineFilter { contains: "common_term_for_filtering".to_string() }),
    };
    let query_source = QuerySource::ByLogQl(query);
    
    let now = Utc::now();
    let start_time = now - ChronoDuration::seconds(3600); // 1 hour ago
    let end_time = now;

    c.bench_function("query_log_range_aggregation_logql_10g_100rpg_3sel_filter", |b| {
        b.iter(|| {
            query_log_range_aggregation(
                black_box(&log_store),
                black_box(query_source.clone()), // Clone query_source for each iteration
                black_box(start_time),
                black_box(end_time),
            )
        })
    });
}

fn benchmark_qra_by_id(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let (log_store, group_ids) = setup_log_store(10, 100, &mut rng);
    
    let target_group_id = group_ids.first().expect("Should have at least one group_id").clone();
    let query_source = QuerySource::ById(target_group_id);

    let now = Utc::now();
    let start_time = now - ChronoDuration::seconds(7200); // 2 hours ago
    let end_time = now;

    c.bench_function("query_log_range_aggregation_by_id_10g_100rpg", |b| {
        b.iter(|| {
            query_log_range_aggregation(
                black_box(&log_store),
                black_box(query_source.clone()), // Clone query_source for each iteration
                black_box(start_time),
                black_box(end_time),
            )
        })
    });
}

criterion_group!(benches, benchmark_execute_logql_query, benchmark_qra_logql, benchmark_qra_by_id);
criterion_main!(benches);
