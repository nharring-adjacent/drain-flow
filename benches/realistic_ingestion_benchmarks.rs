//! Benchmarks the ingestion performance of various log types using different drain backends.
// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use drain_flow::drains::{simple::SingleLayer, two_stage_drain::TwoStageDrain};
use drain_flow::log_group::LogGroup; // Keep if LogGroup is explicitly used, otherwise can be removed if only via LogStore
use drain_flow::query::LogStore;

// Assuming 'generators' is a module in the parent directory (benches/generators/mod.rs)
// or part of the crate structure accessible via `crate::`
use crate::generators::{
    generate_k8s_infra_logs, generate_k8s_mesh_logs, generate_mysql_slow_query_logs,
    generate_rails_app_logs, generate_syslog_messages,
};

// Benchmarks ingestion for MySQL slow query logs.
fn benchmark_mysql_ingestion(c: &mut Criterion) {
    let line_counts = [100, 1000, 5000];
    let seed = 123; // Fixed seed for reproducibility

    // --- Benchmarks for SingleLayer Drain ---

    let mut group_sl_process = c.benchmark_group("MySQL_SingleLayer_ProcessLine");
    for count in line_counts.iter() {
        group_sl_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_mysql_slow_query_logs(*count, seed);
        group_sl_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain =
                        SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_sl_process.finish();

    let mut group_sl_store = c.benchmark_group("MySQL_SingleLayer_LogStorePopulation");
    for count in line_counts.iter() {
        group_sl_store.throughput(Throughput::Elements(*count as u64)); // Using *count as it represents lines for mysql_gen
        let logs = generate_mysql_slow_query_logs(*count, seed);
        group_sl_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain =
                        SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_sl_store.finish();

    // --- Benchmarks for TwoStageDrain ---
    let mut group_tsd_process = c.benchmark_group("MySQL_TwoStageDrain_ProcessLine");
    for count in line_counts.iter() {
        group_tsd_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_mysql_slow_query_logs(*count, seed);
        group_tsd_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
                        .expect("Failed to create TwoStageDrain");
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_tsd_process.finish();

    let mut group_tsd_store = c.benchmark_group("MySQL_TwoStageDrain_LogStorePopulation");
    for count in line_counts.iter() {
        group_tsd_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_mysql_slow_query_logs(*count, seed);
        group_tsd_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
                        .expect("Failed to create TwoStageDrain");
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_tsd_store.finish();
}

// Benchmarks ingestion for Ruby on Rails application logs.
fn benchmark_rails_ingestion(c: &mut Criterion) {
    let line_counts = [100, 1000, 5000]; // For Rails, this is 'num_requests'
    let seed = 123;

    // --- SingleLayer: Rails ---
    let mut group_sl_process = c.benchmark_group("Rails_SingleLayer_ProcessLine");
    for count in line_counts.iter() {
        let logs = generate_rails_app_logs(*count, seed);
        group_sl_process.throughput(Throughput::Elements(logs.len() as u64));
        group_sl_process.bench_with_input(BenchmarkId::from_parameter(count), &logs, |b, l| {
            b.iter(|| {
                let mut drain =
                    SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                for line in l.iter() {
                    drain.process_line(black_box(line.clone()));
                }
            });
        });
    }
    group_sl_process.finish();

    let mut group_sl_store = c.benchmark_group("Rails_SingleLayer_LogStorePopulation");
    for count in line_counts.iter() {
        let logs = generate_rails_app_logs(*count, seed);
        group_sl_store.throughput(Throughput::Elements(logs.len() as u64));
        group_sl_store.bench_with_input(BenchmarkId::from_parameter(count), &logs, |b, l| {
            b.iter(|| {
                let mut drain =
                    SingleLayer::new(vec![]).expect("Failed to create SingleLayer drain");
                for line in l.iter() {
                    drain.process_line(black_box(line.clone()));
                }
                let log_groups = drain.collect_all_log_groups();
                let _store = LogStore::from_log_groups(black_box(log_groups));
            });
        });
    }
    group_sl_store.finish();

    // --- TwoStageDrain: Rails ---
    let mut group_tsd_process = c.benchmark_group("Rails_TwoStageDrain_ProcessLine");
    for count in line_counts.iter() {
        let logs = generate_rails_app_logs(*count, seed);
        group_tsd_process.throughput(Throughput::Elements(logs.len() as u64));
        group_tsd_process.bench_with_input(BenchmarkId::from_parameter(count), &logs, |b, l| {
            b.iter(|| {
                let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
                    .expect("Failed to create TwoStageDrain");
                for line in l.iter() {
                    drain.process_line(black_box(line.clone()));
                }
            });
        });
    }
    group_tsd_process.finish();

    let mut group_tsd_store = c.benchmark_group("Rails_TwoStageDrain_LogStorePopulation");
    for count in line_counts.iter() {
        let logs = generate_rails_app_logs(*count, seed);
        group_tsd_store.throughput(Throughput::Elements(logs.len() as u64));
        group_tsd_store.bench_with_input(BenchmarkId::from_parameter(count), &logs, |b, l| {
            b.iter(|| {
                let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
                    .expect("Failed to create TwoStageDrain");
                for line in l.iter() {
                    drain.process_line(black_box(line.clone()));
                }
                let log_groups = drain.collect_all_log_groups();
                let _store = LogStore::from_log_groups(black_box(log_groups));
            });
        });
    }
    group_tsd_store.finish();
}

// Benchmarks ingestion for Syslog messages.
fn benchmark_syslog_ingestion(c: &mut Criterion) {
    let line_counts = [100, 1000, 5000];
    let seed = 123;

    // --- SingleLayer: Syslog ---
    let mut group_sl_process = c.benchmark_group("Syslog_SingleLayer_ProcessLine");
    for count in line_counts.iter() {
        group_sl_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_syslog_messages(*count, seed);
        group_sl_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = SingleLayer::new(vec![]).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_sl_process.finish();

    let mut group_sl_store = c.benchmark_group("Syslog_SingleLayer_LogStorePopulation");
    for count in line_counts.iter() {
        group_sl_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_syslog_messages(*count, seed);
        group_sl_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = SingleLayer::new(vec![]).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_sl_store.finish();

    // --- TwoStageDrain: Syslog ---
    let mut group_tsd_process = c.benchmark_group("Syslog_TwoStageDrain_ProcessLine");
    for count in line_counts.iter() {
        group_tsd_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_syslog_messages(*count, seed);
        group_tsd_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_tsd_process.finish();

    let mut group_tsd_store = c.benchmark_group("Syslog_TwoStageDrain_LogStorePopulation");
    for count in line_counts.iter() {
        group_tsd_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_syslog_messages(*count, seed);
        group_tsd_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_tsd_store.finish();
}

// Benchmarks ingestion for Kubernetes service mesh (JSON) logs.
fn benchmark_k8s_mesh_ingestion(c: &mut Criterion) {
    let line_counts = [100, 1000, 5000];
    let seed = 123;

    // --- SingleLayer: K8s Mesh ---
    let mut group_sl_process = c.benchmark_group("K8sMesh_SingleLayer_ProcessLine");
    for count in line_counts.iter() {
        group_sl_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_mesh_logs(*count, seed);
        group_sl_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = SingleLayer::new(vec![]).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_sl_process.finish();

    let mut group_sl_store = c.benchmark_group("K8sMesh_SingleLayer_LogStorePopulation");
    for count in line_counts.iter() {
        group_sl_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_mesh_logs(*count, seed);
        group_sl_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = SingleLayer::new(vec![]).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_sl_store.finish();

    // --- TwoStageDrain: K8s Mesh ---
    let mut group_tsd_process = c.benchmark_group("K8sMesh_TwoStageDrain_ProcessLine");
    for count in line_counts.iter() {
        group_tsd_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_mesh_logs(*count, seed);
        group_tsd_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_tsd_process.finish();

    let mut group_tsd_store = c.benchmark_group("K8sMesh_TwoStageDrain_LogStorePopulation");
    for count in line_counts.iter() {
        group_tsd_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_mesh_logs(*count, seed);
        group_tsd_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_tsd_store.finish();
}

// Benchmarks ingestion for Kubernetes infrastructure (klog) logs.
fn benchmark_k8s_infra_ingestion(c: &mut Criterion) {
    let line_counts = [100, 1000, 5000];
    let seed = 123;

    // --- SingleLayer: K8s Infra ---
    let mut group_sl_process = c.benchmark_group("K8sInfra_SingleLayer_ProcessLine");
    for count in line_counts.iter() {
        group_sl_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_infra_logs(*count, seed);
        group_sl_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = SingleLayer::new(vec![]).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_sl_process.finish();

    let mut group_sl_store = c.benchmark_group("K8sInfra_SingleLayer_LogStorePopulation");
    for count in line_counts.iter() {
        group_sl_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_infra_logs(*count, seed);
        group_sl_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = SingleLayer::new(vec![]).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_sl_store.finish();

    // --- TwoStageDrain: K8s Infra ---
    let mut group_tsd_process = c.benchmark_group("K8sInfra_TwoStageDrain_ProcessLine");
    for count in line_counts.iter() {
        group_tsd_process.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_infra_logs(*count, seed);
        group_tsd_process.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                });
            },
        );
    }
    group_tsd_process.finish();

    let mut group_tsd_store = c.benchmark_group("K8sInfra_TwoStageDrain_LogStorePopulation");
    for count in line_counts.iter() {
        group_tsd_store.throughput(Throughput::Elements(*count as u64));
        let logs = generate_k8s_infra_logs(*count, seed);
        group_tsd_store.bench_with_input(
            BenchmarkId::from_parameter(count),
            &logs,
            |b, l: &Vec<String>| {
                b.iter(|| {
                    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).unwrap();
                    for line in l.iter() {
                        drain.process_line(black_box(line.clone()));
                    }
                    let log_groups = drain.collect_all_log_groups();
                    let _store = LogStore::from_log_groups(black_box(log_groups));
                });
            },
        );
    }
    group_tsd_store.finish();
}

criterion_group!(
    benches,
    benchmark_mysql_ingestion,
    benchmark_rails_ingestion, // Added Rails benchmark to the group
    benchmark_syslog_ingestion,
    benchmark_k8s_mesh_ingestion,
    benchmark_k8s_infra_ingestion
);
criterion_main!(benches);
