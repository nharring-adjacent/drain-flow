// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use drain_flow::drains::two_stage_drain::TwoStageDrain;
// LogGroup is used implicitly by LogStore and queries, direct import not always needed for benchmarks.
// use drain_flow::log_group::LogGroup; 
use drain_flow::query::{
    execute_logql_query, query_log_range_aggregation, LineFilter, LogQlQuery, LogStore, QuerySource,
    StreamSelector,
};
// Removed: use drain_flow::collect_log_groups_from_drain; // Replaced with drain.collect_all_log_groups()

// Import all generators
use super::generators::{
    generate_k8s_infra_logs, generate_k8s_mesh_logs, generate_mysql_slow_query_logs,
    generate_rails_app_logs, generate_syslog_messages,
};
use chrono::{Utc, Duration as ChronoDuration};
use regex::Regex; // For Rails Request ID extraction


// Helper function to setup LogStore for MySQL
fn setup_mysql_log_store(num_logs: usize, seed: u64) -> LogStore {
    let logs = generate_mysql_slow_query_logs(num_logs, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for MySQL");

    for line in logs {
        drain.process_line(line);
    }
    
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

// Benchmark Function for MySQL Queries
fn benchmark_mysql_queries(c: &mut Criterion) {
    let num_logs = 5000;
    let seed = 456;
    let store = setup_mysql_log_store(num_logs, seed);

    let base_time = Utc::now() - ChronoDuration::days(15); 
    let start_time = base_time - ChronoDuration::minutes(30);
    let end_time = base_time + ChronoDuration::minutes(30);

    let target_group_id = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(40), Utc::now()) 
        .iter()
        .find(|lg| !lg.get_examples().is_empty() && lg.count > 10 && lg.log_template.contains("Query_time"))
        .map(|lg| lg.id)
        .expect("No suitable log group found for MySQL QRA. Adjust criteria or seed.");

    let mut group_qra = c.benchmark_group("MySQL_QRA_ById_NarrowTime");
    group_qra.bench_function(BenchmarkId::from_parameter(num_logs), |b| {
        b.iter(|| {
            query_log_range_aggregation(
                black_box(&store),
                black_box(QuerySource::ById(target_group_id)),
                black_box(start_time),
                black_box(end_time),
            )
        });
    });
    group_qra.finish();

    let logql_slow = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter {
            contains: "Query_time: 5.".to_string(), 
        }),
    };
    let mut group_logql_slow = c.benchmark_group("MySQL_LogQL_SlowQueries");
    group_logql_slow.bench_function(BenchmarkId::from_parameter(num_logs), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_slow)));
    });
    group_logql_slow.finish();

    let specific_query_text = "SELECT * FROM users WHERE id = ?;".to_string();
    let logql_specific_query = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter {
            contains: specific_query_text,
        }),
    };
    let mut group_logql_specific = c.benchmark_group("MySQL_LogQL_SpecificQuery");
    group_logql_specific.bench_function(BenchmarkId::from_parameter(num_logs), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_specific_query)));
    });
    group_logql_specific.finish();
}

// --- Ruby on Rails ---
fn setup_rails_log_store(num_requests: usize, seed: u64) -> LogStore {
    let logs = generate_rails_app_logs(num_requests, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for Rails");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

fn benchmark_rails_queries(c: &mut Criterion) {
    let num_requests = 1000; // As specified
    let seed = 457;
    
    // Extract a real Request ID
    let sample_logs_for_id = generate_rails_app_logs(1, seed); // Generate logs for one request
    let request_id_re = Regex::new(r"\[REQUEST_ID: ([a-f0-9\-]{12})\]").unwrap(); // Rails gen uses 12 char UUID prefix
    let mut actual_extracted_request_id = String::new();
    for line in sample_logs_for_id.iter().take(5) { // Check first few lines
        if let Some(caps) = request_id_re.captures(line) {
            if let Some(id_match) = caps.get(1) {
                actual_extracted_request_id = id_match.as_str().to_string();
                break;
            }
        }
    }
    if actual_extracted_request_id.is_empty() {
        panic!("Could not extract a Request ID for Rails benchmark. Generator might have changed.");
    }

    let store = setup_rails_log_store(num_requests, seed);

    // Query 1: LogQL for the extracted Request ID
    let logql_request_id = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter {
            contains: format!("[REQUEST_ID: {}]", actual_extracted_request_id),
        }),
    };
    let mut group_logql_req_id = c.benchmark_group("Rails_LogQL_ByRequestID");
    group_logql_req_id.bench_function(BenchmarkId::from_parameter(num_requests), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_request_id)));
    });
    group_logql_req_id.finish();

    // Query 2: LogQL for all ERROR or FATAL messages
    // Rails format: "E, [...] ERROR -- : [...]" or "F, [...] FATAL -- : [...]"
    // A simpler filter for now, can be expanded with regex if LineFilter supports it.
    let logql_errors = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter {
            contains: "] ERROR -- :".to_string(), // Or "FATAL -- :"
        }),
    };
    let mut group_logql_errors = c.benchmark_group("Rails_LogQL_ErrorMessages");
    group_logql_errors.bench_function(BenchmarkId::from_parameter(num_requests), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_errors)));
    });
    group_logql_errors.finish();
    
    // Query 3: QRA for "UsersController#index"
    let base_time = Utc::now() - ChronoDuration::days(1); // Rails logs are generated around Utc::now()
    let start_time = base_time - ChronoDuration::minutes(15);
    let end_time = base_time + ChronoDuration::minutes(15);

    let target_group_id_rails = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(2), Utc::now() + ChronoDuration::days(1))
        .iter()
        .find(|lg| lg.log_template.contains("UsersController#index"))
        .map(|lg| lg.id)
        .expect("No suitable log group for Rails QRA (UsersController#index). Adjust criteria/seed.");
        
    let mut group_qra_rails = c.benchmark_group("Rails_QRA_UsersController");
    group_qra_rails.bench_function(BenchmarkId::from_parameter(num_requests), |b| {
        b.iter(|| {
            query_log_range_aggregation(
                black_box(&store),
                black_box(QuerySource::ById(target_group_id_rails)),
                black_box(start_time),
                black_box(end_time),
            )
        });
    });
    group_qra_rails.finish();
}

// --- Syslog ---
fn setup_syslog_log_store(num_events: usize, seed: u64) -> LogStore {
    let logs = generate_syslog_messages(num_events, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for Syslog");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

fn benchmark_syslog_queries(c: &mut Criterion) {
    let num_events = 5000;
    let seed = 458;
    let store = setup_syslog_log_store(num_events, seed);

    // Query 1: LogQL for messages from `sshd`
    let logql_sshd = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: "sshd[".to_string() }),
    };
    let mut group_logql_sshd = c.benchmark_group("Syslog_LogQL_SshdMessages");
    group_logql_sshd.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_sshd)));
    });
    group_logql_sshd.finish();

    // Query 2: LogQL for "Failed password" messages
    let logql_failed_pw = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: "Failed password for".to_string() }),
    };
    let mut group_logql_failed_pw = c.benchmark_group("Syslog_LogQL_FailedPassword");
    group_logql_failed_pw.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_failed_pw)));
    });
    group_logql_failed_pw.finish();

    // Query 3: QRA for `kernel` messages
    let base_time = Utc::now() - ChronoDuration::days(15); // Syslog gen uses days(rng.gen_range(1..30))
    let start_time = base_time - ChronoDuration::minutes(30);
    let end_time = base_time + ChronoDuration::minutes(30);
    
    let target_group_id_kernel = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(40), Utc::now())
        .iter()
        .find(|lg| lg.log_template.contains("kernel:") && lg.count > 5) // Ensure it's a common kernel group
        .map(|lg| lg.id)
        .expect("No suitable log group for Syslog QRA (kernel). Adjust criteria/seed.");

    let mut group_qra_kernel = c.benchmark_group("Syslog_QRA_KernelMessages");
    group_qra_kernel.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| {
            query_log_range_aggregation(
                black_box(&store),
                black_box(QuerySource::ById(target_group_id_kernel)),
                black_box(start_time),
                black_box(end_time),
            )
        });
    });
    group_qra_kernel.finish();
}

// --- Kubernetes Mesh (JSON) ---
fn setup_k8s_mesh_log_store(num_events: usize, seed: u64) -> LogStore {
    let logs = generate_k8s_mesh_logs(num_events, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for K8s Mesh");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

fn benchmark_k8s_mesh_queries(c: &mut Criterion) {
    let num_events = 5000;
    let seed = 459;
    let store = setup_k8s_mesh_log_store(num_events, seed);

    // Query 1: LogQL for HTTP 5xx errors
    let logql_5xx = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: "\"status_code\":5".to_string() }),
    };
    let mut group_logql_5xx = c.benchmark_group("K8sMesh_LogQL_Http5xx");
    group_logql_5xx.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_5xx)));
    });
    group_logql_5xx.finish();

    // Query 2: LogQL for requests to "product-catalog" (was "my-product-service" in prompt, using actual from gen)
    let logql_upstream = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: "\"upstream_service_name\":\"product-catalog\"".to_string() }),
    };
    let mut group_logql_upstream = c.benchmark_group("K8sMesh_LogQL_UpstreamService");
    group_logql_upstream.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_upstream)));
    });
    group_logql_upstream.finish();
    
    // Query 3: LogQL for requests with high duration (e.g., >= 500ms)
    // k8s_mesh_gen.rs has duration_ms. Searching for "duration_ms": followed by a high hundreds digit.
    // E.g., "duration_ms":5xx, "duration_ms":6xx ...
    // For simplicity, using contains "duration_ms":5, which would match 5, 50-59, 500-599.
    // More specific: "duration_ms": followed by 3 digits starting with 5,6,7,8,9.
    // The generator produces duration_ms up to 500ms, and higher for errors.
    // Let's try to find durations in the several hundreds.
    let logql_high_duration = LogQlQuery {
        selector: StreamSelector::All,
        // This will match "duration_ms":5, "duration_ms":50, "duration_ms":500, etc.
        // It's a broad match. A regex would be better if supported.
        filter: Some(LineFilter { contains: "\"duration_ms\":5".to_string() }), 
    };
    let mut group_logql_high_duration = c.benchmark_group("K8sMesh_LogQL_HighDuration");
    group_logql_high_duration.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_high_duration)));
    });
    group_logql_high_duration.finish();
}

// --- Kubernetes Infra (klog) ---
fn setup_k8s_infra_log_store(num_events: usize, seed: u64) -> LogStore {
    let logs = generate_k8s_infra_logs(num_events, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for K8s Infra");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

fn benchmark_k8s_infra_queries(c: &mut Criterion) {
    let num_events = 5000;
    let seed = 460;
    let store = setup_k8s_infra_log_store(num_events, seed);

    // Query 1a: LogQL for Error (E) level messages
    let logql_errors_klog = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: " E".to_string() }), // Space before E
    };
    let mut group_logql_errors_klog = c.benchmark_group("K8sInfra_LogQL_ErrorLevel");
    group_logql_errors_klog.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_errors_klog)));
    });
    group_logql_errors_klog.finish();

    // Query 1b: LogQL for messages from `pleg.go`
    let logql_pleg = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: " pleg.go".to_string() }),
    };
    let mut group_logql_pleg = c.benchmark_group("K8sInfra_LogQL_PlegMessages");
    group_logql_pleg.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_pleg)));
    });
    group_logql_pleg.finish();

    // Query 2: LogQL for "Failed to schedule pod" messages
    let logql_failed_schedule = LogQlQuery {
        selector: StreamSelector::All,
        filter: Some(LineFilter { contains: "Failed to schedule pod".to_string() }),
    };
    let mut group_logql_failed_schedule = c.benchmark_group("K8sInfra_LogQL_FailedSchedule");
    group_logql_failed_schedule.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_failed_schedule)));
    });
    group_logql_failed_schedule.finish();

    // Query 3: QRA for API server event messages
    // k8s_infra_gen.rs: `Event(v1.ObjectReference...)` comes from kubelet component mostly
    // but API server also logs "Resource event"
    // Let's find a group with "Event(" from kubelet or "event=" from apiserver
    let base_time = Utc::now() - ChronoDuration::days(1); // k8s_infra_gen uses days(rng.gen_range(1..3))
    let start_time = base_time - ChronoDuration::minutes(7) - ChronoDuration::seconds(30); // 15 min window
    let end_time = base_time + ChronoDuration::minutes(7) + ChronoDuration::seconds(30);

    let target_group_id_k8s_event = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(5), Utc::now() + ChronoDuration::days(1))
        .iter()
        .find(|lg| (lg.log_template.contains("Event(") || lg.log_template.contains("event=")) && lg.count > 2) // Ensure common enough
        .map(|lg| lg.id)
        .expect("No suitable log group for K8s Infra QRA (Event). Adjust criteria/seed.");

    let mut group_qra_k8s_event = c.benchmark_group("K8sInfra_QRA_ApiServerEvents");
    group_qra_k8s_event.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| {
            query_log_range_aggregation(
                black_box(&store),
                black_box(QuerySource::ById(target_group_id_k8s_event)),
                black_box(start_time),
                black_box(end_time),
            )
        });
    });
    group_qra_k8s_event.finish();
}


criterion_group!(
    benches,
    benchmark_mysql_queries,
    benchmark_rails_queries,
    benchmark_syslog_queries,
    benchmark_k8s_mesh_queries,
    benchmark_k8s_infra_queries
);
criterion_main!(benches);
