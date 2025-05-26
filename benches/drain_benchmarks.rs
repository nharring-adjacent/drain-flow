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
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use drain_flow::{
    drains::{simple::SingleLayer, two_stage_drain::TwoStageDrain},
    log_group::LogGroup,
    query::{query_log_range_aggregation, LogStore},
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
        let status: usize = rng.gen_range(200..600);
        let message_length: usize = rng.gen_range(5..20);
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789";
        let message: String = (0..message_length)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        let template = RecordTemplate::Sendmail(Sendmail {
            ts: current_time.to_rfc3339(),
            remote: format!("host{}.example.com", rng.gen_range(1..100)),
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
            let mut drain = SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
            b.iter(|| {
                for line in &lines {
                    drain.process_line(black_box(line.clone())).unwrap();
                }
            });
        });
    }
    group.finish();
}

fn benchmark_log_store_from_single_layer(c: &mut Criterion) {
    let mut group = c.benchmark_group("SingleLayer_LogStorePopulation");
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter(|| {
                let mut drain =
                    SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                for line in &lines {
                    drain.process_line(line.clone()).unwrap();
                }
                let log_groups_nested: Vec<Vec<&LogGroup>> = drain.iter_groups(); // Removed .collect()
                let log_groups: Vec<LogGroup> = log_groups_nested
                    .into_iter()
                    .flatten()
                    .map(|lg_ref| lg_ref.clone()) // Clone to get owned LogGroup
                    .collect();
                let _store = LogStore::from_log_groups(black_box(log_groups));
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
        drain.process_line(line.clone()).unwrap();
    }
    let log_groups_nested: Vec<Vec<&LogGroup>> = drain.iter_groups(); // Removed .collect()
    let log_groups: Vec<LogGroup> = log_groups_nested
        .into_iter()
        .flatten()
        .map(|lg_ref| lg_ref.clone())
        .collect();
    let store = LogStore::from_log_groups(log_groups);

    // Find a group with examples for querying
    let target_group_id_opt = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now())
        .iter()
        .find(|lg| !lg.get_examples().is_empty())
        .map(|lg| lg.id);

    if target_group_id_opt.is_none() {
        println!("Warning: No suitable LogGroup with examples found for single_layer query benchmark. Skipping.");
        return;
    }
    let target_group_id = target_group_id_opt.unwrap();

    let base_time = Utc::now() - ChronoDuration::milliseconds(line_count as i64 / 2); // Middle of generated time range

    let time_ranges = [
        (
            base_time - ChronoDuration::milliseconds(500),
            base_time + ChronoDuration::milliseconds(500),
        ), // 1s window
        (
            base_time - ChronoDuration::milliseconds(100),
            base_time + ChronoDuration::milliseconds(100),
        ), // 200ms window
        (base_time, base_time + ChronoDuration::milliseconds(50)), // 50ms window
    ];

    for (idx, (start_time, end_time)) in time_ranges.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("QueryTimeRange", format!("Range{}", idx)),
            &(*start_time, *end_time),
            |b, &(s, e)| {
                b.iter(|| {
                    query_log_range_aggregation(
                        black_box(&store),
                        black_box(target_group_id),
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
            // Default params: domain_regex_strings: vec![], threshold: 0.5, max_depth: 4, max_children: 10
            let mut drain =
                TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain");
            b.iter(|| {
                for line in &lines {
                    drain.process_line(black_box(line.clone())).unwrap();
                }
            });
        });
    }
    group.finish();
}

fn benchmark_log_store_from_two_stage_drain(c: &mut Criterion) {
    let mut group = c.benchmark_group("TwoStageDrain_LogStorePopulation");
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            b.iter(|| {
                let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
                    .expect("Failed to create TwoStageDrain");
                for line in &lines {
                    drain.process_line(line.clone()).unwrap();
                }
                let log_groups: Vec<LogGroup> = drain.collect_all_log_groups(); // Using the new method
                let _store = LogStore::from_log_groups(black_box(log_groups));
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
        drain.process_line(line.clone()).unwrap();
    }
    let log_groups = drain.collect_all_log_groups();
    let store = LogStore::from_log_groups(log_groups);

    let target_group_id_opt = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now())
        .iter()
        .find(|lg| !lg.get_examples().is_empty())
        .map(|lg| lg.id);

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
                        black_box(target_group_id),
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
    benchmark_log_store_from_single_layer,
    benchmark_query_on_single_layer_data,
    benchmark_two_stage_drain_process_line,
    benchmark_log_store_from_two_stage_drain,
    benchmark_query_on_two_stage_drain_data
);
criterion_main!(benches);
