// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use chrono::{Duration as ChronoDuration, Utc};
use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
}; // Added BatchSize
use drain_flow::{
    drains::{api::Drain, simple::SingleLayer, two_stage_drain::TwoStageDrain, DifferentialDrain}, // Added DifferentialDrain
    query::{query_log_range_aggregation, LogStore, QuerySource}, // Added QuerySource
                                                                 // record::Record, // Removed as it's unused directly in this file
};
// use rand::distributions::Alphanumeric; // Removed problematic import
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
// use std::{thread::sleep, time::Duration as StdDuration}; // Removed as sleep is not used here

mod generators;
use generators::{LogGenerator, RecordTemplate, Sendmail};

fn generate_log_lines(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let log_generator = LogGenerator::new().expect("Failed to create LogGenerator");
    let base_time = Utc::now();
    let mut lines = Vec::with_capacity(count);

    for i in 0..count {
        let current_time = base_time + ChronoDuration::milliseconds(i as i64);
        // Add some random variation to status and message to create more diverse log groups
        let status: usize = rng.random_range(200_usize..600_usize); // Updated gen_range
        let message_length: usize = rng.random_range(5_usize..20_usize); // Updated gen_range
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789";
        let message: String = (0..message_length)
            .map(|_| {
                let idx = rng.random_range(0_usize..CHARSET.len()); // Updated gen_range
                CHARSET[idx] as char
            })
            .collect();

        let template = RecordTemplate::Sendmail(Sendmail {
            ts: current_time.to_rfc3339(),
            remote: format!("host{}.example.com", rng.random_range(1_usize..100_usize)), // Updated gen_range
            status,
            message,
        });
        lines.push(log_generator.make_record(template));
    }
    lines
}

// --- SingleLayer Benchmarks ---

fn benchmark_single_layer_process_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("SingleLayer_ProcessLine");
    let line_counts = [100, 1000, 5000];
    let seed = 42; // For reproducibility

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            // Setup for each batch, not for each iteration
            b.iter_batched(
                || {
                    (
                        SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain"),
                        lines.clone(),
                    )
                },
                |(mut drain, current_lines)| {
                    for line in current_lines {
                        // Use cloned lines for this batch
                        Drain::process_line(&mut drain, black_box(line)).unwrap();
                    }
                },
                BatchSize::SmallInput, // Assuming setup is relatively cheap
            );
        });
    }
    group.finish();
}

fn benchmark_single_layer_collect_groups_and_create_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("SingleLayer_CollectAndStore");
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter(|| {
                let mut drain_for_iter =
                    SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                for line in &lines {
                    Drain::process_line(&mut drain_for_iter, line.clone()).unwrap();
                }
                // let log_groups = drain_for_iter.collect_log_groups(); // No longer needed here
                let _store = LogStore::new(drain_for_iter); // Pass the drain itself
            });
        });
    }
    group.finish();
}

fn benchmark_query_on_single_layer_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("SingleLayer_Query");
    let line_count = 2000;
    let seed = 42;
    let lines = generate_log_lines(line_count, seed);

    let mut drain = SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
    for line in &lines {
        Drain::process_line(&mut drain, line.clone()).unwrap();
    }
    let store = LogStore::new(drain); // Pass the drain itself

    let available_groups =
        store.get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now()); // Added None for query_id
    let target_group_id_opt = available_groups.get(0).map(|lg_ref| lg_ref.id);

    if target_group_id_opt.is_none() {
        println!("Warning: No suitable LogGroup with examples found for single_layer query benchmark. Skipping.");
        return;
    }
    let target_group_id = target_group_id_opt.unwrap();

    let base_time = Utc::now() - ChronoDuration::milliseconds(line_count as i64 / 2);

    let time_ranges = [
        (
            base_time - ChronoDuration::milliseconds(500),
            base_time + ChronoDuration::milliseconds(500),
        ),
        (
            base_time - ChronoDuration::milliseconds(100),
            base_time + ChronoDuration::milliseconds(100),
        ),
        (base_time, base_time + ChronoDuration::milliseconds(50)),
    ];

    for (idx, (start_time, end_time)) in time_ranges.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("QueryTimeRange", format!("Range{}", idx)),
            &(*start_time, *end_time),
            |b, &(s, e)| {
                b.iter(|| {
                    query_log_range_aggregation(
                        black_box(&store),
                        black_box(QuerySource::ById(target_group_id)), // Use QuerySource enum
                        black_box(s),
                        black_box(e),
                    );
                });
            },
        );
    }
    group.finish();
}

// --- TwoStageDrain Benchmarks ---

fn benchmark_two_stage_drain_process_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("TwoStageDrain_ProcessLine");
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter_batched(
                || {
                    (
                        TwoStageDrain::new(vec![], 0.5, 4, 100)
                            .expect("Failed to create TwoStageDrain"),
                        lines.clone(),
                    )
                },
                |(mut drain, current_lines)| {
                    for line in current_lines {
                        Drain::process_line(&mut drain, black_box(line)).unwrap();
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn benchmark_two_stage_drain_collect_groups_and_create_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("TwoStageDrain_CollectAndStore");
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter(|| {
                let mut drain_for_iter = TwoStageDrain::new(vec![], 0.5, 4, 100)
                    .expect("Failed to create TwoStageDrain");
                for line in &lines {
                    Drain::process_line(&mut drain_for_iter, line.clone()).unwrap();
                }
                // let log_groups = drain_for_iter.collect_log_groups(); // No longer needed here
                let _store = LogStore::new(drain_for_iter); // Pass the drain itself
            });
        });
    }
    group.finish();
}

fn benchmark_query_on_two_stage_drain_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("TwoStageDrain_Query");
    let line_count = 2000;
    let seed = 42;
    let lines = generate_log_lines(line_count, seed);

    let mut drain =
        TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain");
    for line in &lines {
        Drain::process_line(&mut drain, line.clone()).unwrap();
    }
    let store = LogStore::new(drain); // Pass the drain itself

    let available_groups =
        store.get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now()); // Added None for query_id
    let target_group_id_opt = available_groups.get(0).map(|lg_ref| lg_ref.id);

    if target_group_id_opt.is_none() {
        println!("Warning: No suitable LogGroup with examples found for two_stage_drain query benchmark. Skipping.");
        return;
    }
    let target_group_id = target_group_id_opt.unwrap();
    let base_time = Utc::now() - ChronoDuration::milliseconds(line_count as i64 / 2);

    let time_ranges = [
        (
            base_time - ChronoDuration::milliseconds(500),
            base_time + ChronoDuration::milliseconds(500),
        ),
        (
            base_time - ChronoDuration::milliseconds(100),
            base_time + ChronoDuration::milliseconds(100),
        ),
        (base_time, base_time + ChronoDuration::milliseconds(50)),
    ];

    for (idx, (start_time, end_time)) in time_ranges.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("QueryTimeRange", format!("Range{}", idx)),
            &(*start_time, *end_time),
            |b, &(s, e)| {
                b.iter(|| {
                    query_log_range_aggregation(
                        black_box(&store),
                        black_box(QuerySource::ById(target_group_id)), // Use QuerySource enum
                        black_box(s),
                        black_box(e),
                    );
                });
            },
        );
    }
    group.finish();
}

// --- DifferentialDrain Benchmarks ---

fn benchmark_differential_drain_process_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("DifferentialDrain_ProcessLine");
    let line_counts = [100, 1000, 5000]; // Consistent line counts
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter_batched(
                || {
                    // Setup: create a new drain for each iteration batch
                    (DifferentialDrain::new(0.5, 4), lines.clone())
                },
                |(mut drain, current_lines)| {
                    // Action: process each line
                    for line in current_lines {
                        drain.process_line(black_box(line)).unwrap();
                    }
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn benchmark_differential_drain_collect_groups_and_create_store(c: &mut Criterion) {
    let mut group = c.benchmark_group("DifferentialDrain_CollectAndStore");
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter(|| {
                let mut drain_for_iter = DifferentialDrain::new(0.5, 4);
                for line in &lines {
                    drain_for_iter.process_line(line.clone()).unwrap();
                }
                // let log_groups = drain_for_iter.collect_log_groups(); // No longer needed here
                let _store = LogStore::new(drain_for_iter); // Pass the drain itself
            });
        });
    }
    group.finish();
}

fn benchmark_query_on_differential_drain_data(c: &mut Criterion) {
    let mut group = c.benchmark_group("DifferentialDrain_Query");
    let line_count = 2000;
    let seed = 42;
    let lines = generate_log_lines(line_count, seed);

    let mut drain = DifferentialDrain::new(0.5, 4);
    for line in &lines {
        drain.process_line(line.clone()).unwrap();
    }
    let store = LogStore::new(drain); // Pass the drain itself

    let available_groups =
        store.get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now());
    let target_group_id_opt = available_groups.get(0).map(|lg_ref| lg_ref.id);

    if target_group_id_opt.is_none() {
        println!("Warning: No suitable LogGroup with examples found for DifferentialDrain query benchmark. Skipping.");
        return;
    }
    let target_group_id = target_group_id_opt.unwrap();
    let base_time = Utc::now() - ChronoDuration::milliseconds(line_count as i64 / 2);

    let time_ranges = [
        (
            base_time - ChronoDuration::milliseconds(500),
            base_time + ChronoDuration::milliseconds(500),
        ),
        (
            base_time - ChronoDuration::milliseconds(100),
            base_time + ChronoDuration::milliseconds(100),
        ),
        (base_time, base_time + ChronoDuration::milliseconds(50)),
    ];

    for (idx, (start_time, end_time)) in time_ranges.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("QueryTimeRange", format!("Range{}", idx)),
            &(*start_time, *end_time),
            |b, &(s, e)| {
                b.iter(|| {
                    query_log_range_aggregation(
                        black_box(&store),
                        black_box(QuerySource::ById(target_group_id)),
                        black_box(s),
                        black_box(e),
                    );
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    benchmark_single_layer_process_line,
    benchmark_single_layer_collect_groups_and_create_store,
    benchmark_query_on_single_layer_data,
    benchmark_two_stage_drain_process_line,
    benchmark_two_stage_drain_collect_groups_and_create_store,
    benchmark_query_on_two_stage_drain_data,
    benchmark_differential_drain_process_line,
    benchmark_differential_drain_collect_groups_and_create_store,
    benchmark_query_on_differential_drain_data
);
criterion_main!(benches);
