//! Benchmarks query performance against realistic log datasets for various log types.
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
    execute_logql_query, query_log_range_aggregation, LineFilter, LogQlQuery, LogStore,
    QuerySource, StreamSelector,
};
// Removed: use drain_flow::collect_log_groups_from_drain; // Replaced with drain.collect_all_log_groups()

// Import all generators
use crate::generators::{
    // Assuming 'crate::' is the correct path from previous fixes
    generate_k8s_infra_logs,
    generate_k8s_mesh_logs,
    generate_mysql_slow_query_logs,
    generate_rails_app_logs,
    generate_syslog_messages,
};
use chrono::{Duration as ChronoDuration, Utc};
use regex::Regex; // For Rails Request ID extraction

// Helper function to setup LogStore for MySQL
fn setup_mysql_log_store(num_logs: usize, seed: u64) -> LogStore {
    let logs = generate_mysql_slow_query_logs(num_logs, seed);
    let mut drain =
        TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for MySQL");

    for line in logs {
        drain.process_line(line);
    }

    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

// Benchmarks queries against MySQL slow query log data.
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
        .find(|lg| {
            !lg.get_examples().is_empty()
                && lg.count() > 10
                && lg.log_template().contains("Query_time")
        })
        .map(|lg| lg.id())
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
        selector: StreamSelector::default(),
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
        selector: StreamSelector::default(),
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

// Helper function to setup LogStore for Rails
fn setup_rails_log_store(num_requests: usize, seed: u64) -> LogStore {
    let logs = generate_rails_app_logs(num_requests, seed);
    let mut drain =
        TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for Rails");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

// Benchmarks queries against Ruby on Rails application log data.
fn benchmark_rails_queries(c: &mut Criterion) {
    let num_requests = 1000;
    let seed = 457;

    let sample_logs_for_id = generate_rails_app_logs(1, seed);
    let request_id_re = Regex::new(r"\[REQUEST_ID: ([a-f0-9\-]{12})\]").unwrap();
    let mut actual_extracted_request_id = String::new();
    for line in sample_logs_for_id.iter().take(5) {
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

    let logql_request_id = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: format!("[REQUEST_ID: {}]", actual_extracted_request_id),
        }),
    };
    let mut group_logql_req_id = c.benchmark_group("Rails_LogQL_ByRequestID");
    group_logql_req_id.bench_function(BenchmarkId::from_parameter(num_requests), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_request_id)));
    });
    group_logql_req_id.finish();

    let logql_errors = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "] ERROR -- :".to_string(),
        }),
    };
    let mut group_logql_errors = c.benchmark_group("Rails_LogQL_ErrorMessages");
    group_logql_errors.bench_function(BenchmarkId::from_parameter(num_requests), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_errors)));
    });
    group_logql_errors.finish();

    let base_time = Utc::now() - ChronoDuration::days(1);
    let start_time = base_time - ChronoDuration::minutes(15);
    let end_time = base_time + ChronoDuration::minutes(15);

    let target_group_id_rails = store
        .get_log_groups_in_range(
            Utc::now() - ChronoDuration::days(2),
            Utc::now() + ChronoDuration::days(1),
        )
        .iter()
        .find(|lg| lg.log_template().contains("UsersController#index"))
        .map(|lg| lg.id())
        .expect(
            "No suitable log group for Rails QRA (UsersController#index). Adjust criteria/seed.",
        );

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

// Helper function to setup LogStore for Syslog
fn setup_syslog_log_store(num_events: usize, seed: u64) -> LogStore {
    let logs = generate_syslog_messages(num_events, seed);
    let mut drain =
        TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain for Syslog");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

// Benchmarks queries against Syslog data.
fn benchmark_syslog_queries(c: &mut Criterion) {
    let num_events = 5000;
    let seed = 458;
    let store = setup_syslog_log_store(num_events, seed);

    let logql_sshd = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "sshd[".to_string(),
        }),
    };
    let mut group_logql_sshd = c.benchmark_group("Syslog_LogQL_SshdMessages");
    group_logql_sshd.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_sshd)));
    });
    group_logql_sshd.finish();

    let logql_failed_pw = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "Failed password for".to_string(),
        }),
    };
    let mut group_logql_failed_pw = c.benchmark_group("Syslog_LogQL_FailedPassword");
    group_logql_failed_pw.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_failed_pw)));
    });
    group_logql_failed_pw.finish();

    let base_time = Utc::now() - ChronoDuration::days(15);
    let start_time = base_time - ChronoDuration::minutes(30);
    let end_time = base_time + ChronoDuration::minutes(30);

    let target_group_id_kernel = store
        .get_log_groups_in_range(Utc::now() - ChronoDuration::days(40), Utc::now())
        .iter()
        .find(|lg| lg.log_template().contains("kernel:") && lg.count() > 5)
        .map(|lg| lg.id())
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

// Helper function to setup LogStore for K8s Mesh (JSON)
fn setup_k8s_mesh_log_store(num_events: usize, seed: u64) -> LogStore {
    let logs = generate_k8s_mesh_logs(num_events, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
        .expect("Failed to create TwoStageDrain for K8s Mesh");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

// Benchmarks queries against Kubernetes service mesh (JSON) log data.
fn benchmark_k8s_mesh_queries(c: &mut Criterion) {
    let num_events = 5000;
    let seed = 459;
    let store = setup_k8s_mesh_log_store(num_events, seed);

    let logql_5xx = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "\"status_code\":5".to_string(),
        }),
    };
    let mut group_logql_5xx = c.benchmark_group("K8sMesh_LogQL_Http5xx");
    group_logql_5xx.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_5xx)));
    });
    group_logql_5xx.finish();

    let logql_upstream = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "\"upstream_service_name\":\"product-catalog\"".to_string(),
        }),
    };
    let mut group_logql_upstream = c.benchmark_group("K8sMesh_LogQL_UpstreamService");
    group_logql_upstream.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_upstream)));
    });
    group_logql_upstream.finish();

    let logql_high_duration = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "\"duration_ms\":5".to_string(),
        }),
    };
    let mut group_logql_high_duration = c.benchmark_group("K8sMesh_LogQL_HighDuration");
    group_logql_high_duration.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_high_duration)));
    });
    group_logql_high_duration.finish();
}

// Helper function to setup LogStore for K8s Infra (klog)
fn setup_k8s_infra_log_store(num_events: usize, seed: u64) -> LogStore {
    let logs = generate_k8s_infra_logs(num_events, seed);
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 100)
        .expect("Failed to create TwoStageDrain for K8s Infra");
    for line in logs {
        drain.process_line(line);
    }
    let log_groups = drain.collect_all_log_groups();
    LogStore::from_log_groups(log_groups)
}

// Benchmarks queries against Kubernetes infrastructure (klog) log data.
fn benchmark_k8s_infra_queries(c: &mut Criterion) {
    let num_events = 5000;
    let seed = 460;
    let store = setup_k8s_infra_log_store(num_events, seed);

    let logql_errors_klog = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: " E".to_string(),
        }),
    };
    let mut group_logql_errors_klog = c.benchmark_group("K8sInfra_LogQL_ErrorLevel");
    group_logql_errors_klog.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_errors_klog)));
    });
    group_logql_errors_klog.finish();

    let logql_pleg = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: " pleg.go".to_string(),
        }),
    };
    let mut group_logql_pleg = c.benchmark_group("K8sInfra_LogQL_PlegMessages");
    group_logql_pleg.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_pleg)));
    });
    group_logql_pleg.finish();

    let logql_failed_schedule = LogQlQuery {
        selector: StreamSelector::default(),
        filter: Some(LineFilter {
            contains: "Failed to schedule pod".to_string(),
        }),
    };
    let mut group_logql_failed_schedule = c.benchmark_group("K8sInfra_LogQL_FailedSchedule");
    group_logql_failed_schedule.bench_function(BenchmarkId::from_parameter(num_events), |b| {
        b.iter(|| execute_logql_query(black_box(&store), black_box(&logql_failed_schedule)));
    });
    group_logql_failed_schedule.finish();

    let base_time = Utc::now() - ChronoDuration::days(1);
    let start_time = base_time - ChronoDuration::minutes(7) - ChronoDuration::seconds(30);
    let end_time = base_time + ChronoDuration::minutes(7) + ChronoDuration::seconds(30);

    let target_group_id_k8s_event = store
        .get_log_groups_in_range(
            Utc::now() - ChronoDuration::days(5),
            Utc::now() + ChronoDuration::days(1),
        )
        .iter()
        .find(|lg| {
            (lg.log_template().contains("Event(") || lg.log_template().contains("event="))
                && lg.count() > 2
        })
        .map(|lg| lg.id())
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
