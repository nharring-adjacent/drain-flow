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
    drains::{api::Drain, simple::SingleLayer, two_stage_drain::TwoStageDrain}, // Added api::Drain
    // log_group::LogGroup, // Removed as per compiler warning (unused)
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
        let status: usize = rng.random_range(200_usize..600_usize);
        let message_length: usize = rng.random_range(5_usize..20_usize);
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                                abcdefghijklmnopqrstuvwxyz\
                                0123456789";
        let message: String = (0..message_length)
            .map(|_| {
                let idx = rng.random_range(0_usize..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();

        let template = RecordTemplate::Sendmail(Sendmail {
            ts: current_time.to_rfc3339(),
            remote: format!("host{}.example.com", rng.random_range(1_usize..100_usize)),
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
                    Drain::process_line(&mut drain, black_box(line.clone())).unwrap(); // Updated call
                }
            });
        });
    }
    group.finish();
}

fn benchmark_single_layer_collect_groups_and_create_store(c: &mut Criterion) { // Renamed
    let mut group = c.benchmark_group("SingleLayer_CollectAndStore"); // Updated group name
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            // Drain population is part of the setup for this specific benchmark iteration
            // but not part of the b.iter() loop, to focus on collection + store creation.
            // If drain population needs to be benchmarked *with* collection, it should be inside b.iter().
            // For now, assuming we benchmark collection and store creation on a pre-populated drain.
            let mut drain =
                SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
            for line in &lines {
                Drain::process_line(&mut drain, line.clone()).unwrap();
            }

            b.iter(|| {
                // In each iteration, collect groups and create the store.
                // This means drain is cloned or re-populated if we want to measure population too.
                // The current setup reuses the populated drain from outside b.iter,
                // which means we are only measuring collect_log_groups and LogStore::new.
                // To be consistent with "populating the drain as it does currently" *inside* the benchmarked loop:
                // We need to decide if drain creation & population is part of what's measured for "LogStorePopulation"
                // The original name `benchmark_log_store_from_single_layer` implied the whole process.
                // Let's assume the intent is to benchmark the collection and store creation part primarily.
                // So, the drain is populated once, then we benchmark collection + store creation.
                // If drain state changes or is consumed by LogStore::new, then drain must be setup inside b.iter.
                // Since LogStore::new(drain) takes ownership, drain needs to be setup inside.

                let mut drain_for_iter =
                    SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                for line in &lines { // Populate drain inside iter
                    Drain::process_line(&mut drain_for_iter, line.clone()).unwrap();
                }

                let _log_groups = drain_for_iter.collect_log_groups(); // Collect groups
                let _store = LogStore::new(black_box(drain_for_iter)); // Create store
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
    // LogStore is now created with the drain instance.
    // The drain is consumed by LogStore::new, so if drain is needed later, it must be cloned.
    // For this benchmark, we populate the drain, then pass it to LogStore.
    let store = LogStore::new(drain); // drain is moved here.

    // Find a group with examples for querying
    // get_log_groups_in_range now returns Vec<LogGroup> (owned)
    let available_groups = store.get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now());
    let target_group_id_opt = available_groups
        .iter() // Iterate over &LogGroup
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
                        black_box(drain_flow::query::QuerySource::ById(target_group_id)),
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
                    Drain::process_line(&mut drain, black_box(line.clone())).unwrap(); // Updated call
                }
            });
        });
    }
    group.finish();
}

fn benchmark_two_stage_drain_collect_groups_and_create_store(c: &mut Criterion) { // Renamed
    let mut group = c.benchmark_group("TwoStageDrain_CollectAndStore"); // Updated group name
    let line_counts = [100, 1000, 5000];
    let seed = 42;

    for count in line_counts.iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &size| {
            let lines = generate_log_lines(size, seed);
            // Similar to SingleLayer, setting up drain inside b.iter for consistency
            b.iter(|| {
                let mut drain_for_iter = TwoStageDrain::new(vec![], 0.5, 4, 100)
                    .expect("Failed to create TwoStageDrain");
                for line in &lines { // Populate drain inside iter
                    Drain::process_line(&mut drain_for_iter, line.clone()).unwrap();
                }
                let _log_groups = drain_for_iter.collect_log_groups(); // Collect groups
                let _store = LogStore::new(black_box(drain_for_iter));  // Create store
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
    let store = LogStore::new(drain); // drain is moved here

    let available_groups = store.get_log_groups_in_range(Utc::now() - ChronoDuration::days(365), Utc::now());
    let target_group_id_opt = available_groups
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
                        black_box(drain_flow::query::QuerySource::ById(target_group_id)),
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
    benchmark_single_layer_collect_groups_and_create_store, // Renamed
    benchmark_query_on_single_layer_data,
    benchmark_two_stage_drain_process_line,
    benchmark_two_stage_drain_collect_groups_and_create_store, // Renamed
    benchmark_query_on_two_stage_drain_data
);
criterion_main!(benches);
