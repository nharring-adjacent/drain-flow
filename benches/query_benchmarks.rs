use chrono::{Duration as ChronoDuration, Utc};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use drain_flow::{
    drains::{api::Drain, simple::SingleLayer}, // Added Drain and SingleLayer
    // log_group::LogGroup, // Removed as unused
    query::{
        execute_logql_query, query_log_range_aggregation, LineFilter, LogQlQuery, LogStore,
        QuerySource, StreamSelector,
    },
    // record::Record, // Removed as unused
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use uuid::Uuid;

// Helper to create a LogStore with a specific number of groups and records
// This function now populates a SingleLayer drain by processing lines.
fn setup_log_store(
    num_groups: usize,
    records_per_group: usize,
    rng: &mut StdRng,
) -> (LogStore<SingleLayer>, Vec<Uuid>) {
    let mut drain = SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");

    for i in 0..num_groups {
        // Create a unique base log line for each group
        // Ensure this line is distinct enough to form its own group in SingleLayer if threshold is high enough,
        // or similar enough if we want fewer groups. For simplicity, assume they form distinct groups.
        let base_line = format!(
            "Base log line for group {} {} {}",
            i,
            Uuid::from_u128(rng.random()), // Add UUID to ensure uniqueness for base
            // Add some variable tokens that SingleLayer might use for grouping if not unique enough
            (0..rng.random_range(3..7)) // Replaced gen_range
                .map(|_| format!("token{}", rng.random_range(0..5))) // Replaced gen_range
                .collect::<Vec<_>>()
                .join(" ")
        );
        Drain::process_line(&mut drain, base_line.clone()).expect("Failed to process base line");

        // Generate example lines for this group
        for j in 0..records_per_group {
            // A better example line generation that is similar to base_line for SingleLayer:
            let mut base_tokens: Vec<&str> = base_line.split_whitespace().collect();
            let tokens_to_change = rng.random_range(1..std::cmp::max(2, base_tokens.len() / 2)); // Replaced gen_range
            for _ in 0..tokens_to_change {
                if !base_tokens.is_empty() {
                    let idx_to_change = rng.random_range(0..base_tokens.len()); // Replaced gen_range
                    base_tokens[idx_to_change] =
                        if j % 2 == 0 && idx_to_change == base_tokens.len() - 1 {
                            // Corrected index check
                            "common_term_for_filtering"
                        } else {
                            // This needs a static string or a pool, can't use format! directly here easily
                            // For simplicity, let's just append.
                            "changed_token" // This was "changed_token"
                        };
                }
            }
            let mut modified_example_line = base_tokens.join(" ");
            if j % 2 == 0 {
                // ensure common_term_for_filtering is present in some
                if !modified_example_line.contains("common_term_for_filtering") {
                    // Avoid duplicating if already changed
                    modified_example_line.push_str(" common_term_for_filtering");
                }
            }
            modified_example_line
                .push_str(&format!(" example_id_{}", Uuid::from_u128(rng.random())));

            Drain::process_line(&mut drain, modified_example_line)
                .expect("Failed to process example line");
        }
    }

    let formed_log_groups = drain.collect_log_groups(); // This uses LogGroup from drain_flow::log_group
    let group_ids: Vec<Uuid> = formed_log_groups.into_iter().map(|lg| lg.id).collect();

    (LogStore::new(drain), group_ids)
}

fn benchmark_execute_logql_query(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let (log_store, group_ids) = setup_log_store(10, 100, &mut rng);

    // Ensure there are group_ids to select from, otherwise skip.
    if group_ids.is_empty() {
        println!("Warning: No log groups formed in setup_log_store for benchmark_execute_logql_query. Skipping.");
        return;
    }

    let query = LogQlQuery {
        selector: StreamSelector::LogGroupIds(group_ids.iter().take(5).cloned().collect()), // Select first 5 groups
        filter: Some(LineFilter {
            contains: "common_term_for_filtering".to_string(),
        }),
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
        filter: Some(LineFilter {
            contains: "common_term_for_filtering".to_string(),
        }),
    };
    let query_source = QuerySource::ByLogQl(query);

    let now = Utc::now();
    let start_time = now - ChronoDuration::seconds(3600); // 1 hour ago
    let end_time = now;

    c.bench_function(
        "query_log_range_aggregation_logql_10g_100rpg_3sel_filter",
        |b| {
            b.iter(|| {
                query_log_range_aggregation(
                    black_box(&log_store),
                    black_box(query_source.clone()), // Clone query_source for each iteration
                    black_box(start_time),
                    black_box(end_time),
                )
            })
        },
    );
}

fn benchmark_qra_by_id(c: &mut Criterion) {
    let mut rng = StdRng::seed_from_u64(42);
    let (log_store, group_ids) = setup_log_store(10, 100, &mut rng);

    let target_group_id = *group_ids
        .first()
        .expect("Should have at least one group_id");
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

criterion_group!(
    benches,
    benchmark_execute_logql_query,
    benchmark_qra_logql,
    benchmark_qra_by_id
);
criterion_main!(benches);
