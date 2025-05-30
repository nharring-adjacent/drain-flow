// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

//! Generates realistic JSON-formatted logs for Kubernetes service mesh traffic (e.g., Istio, Linkerd, Nginx Ingress).

use rand::prelude::IndexedRandom;
use rand::{rngs::StdRng, Rng, SeedableRng}; // For .choose()
                                            // Removed: use rand::seq::SliceRandom;
use chrono::{Duration, SecondsFormat, Utc};
//use serde::Serialize; // The trait
use serde_derive::Serialize; // The derive macro
use serde_json;
use uuid::Uuid;

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
pub struct K8sMeshLogEntry {
    timestamp: String,
    level: String,
    request_id: String,
    method: String,
    path: String,
    protocol: String,
    status_code: u16,
    bytes_sent: u64,
    bytes_received: u64,
    duration_ms: u32,
    upstream_service_name: String,
    upstream_service_namespace: String,
    upstream_cluster: String,
    upstream_host_pod_ip: String,
    downstream_remote_address: String,
    downstream_local_address_pod_ip: String,
    authority: String,
    user_agent: String,
    x_forwarded_for: Option<String>,
    response_flags: String,
    tls_version: Option<String>,
    tls_cipher: Option<String>,
}

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
const COMMON_PATHS_PREFIX: &[&str] = &["/api/v1", "/api/v2", "/app", "/service", "/data"];
const COMMON_PATHS_RESOURCE: &[&str] = &[
    "users", "products", "orders", "items", "metrics", "status", "config",
];
const COMMON_PATHS_SUFFIX: &[&str] = &["", "/:id", "/:id/summary", "/search", "/stream"];
const QUERY_PARAMS_KEYS: &[&str] = &[
    "session_id",
    "user_token",
    "format",
    "limit",
    "offset",
    "debug",
];
const PROTOCOLS: &[&str] = &["HTTP/1.1", "HTTP/2.0"];
const STATUS_CODES_WEIGHTED: &[(u16, usize)] = &[
    (200, 60),
    (201, 10),
    (204, 5),
    (301, 2),
    (302, 2),
    (304, 3),
    (400, 3),
    (401, 2),
    (403, 2),
    (404, 5),
    (429, 1),
    (500, 3),
    (502, 1),
    (503, 1),
    (504, 1),
];
const UPSTREAM_SERVICE_NAMES: &[&str] = &[
    "auth-service",
    "product-catalog",
    "order-processor",
    "user-profile",
    "inventory-cache",
];
const UPSTREAM_SERVICE_NAMESPACES: &[&str] =
    &["prod-ns", "staging-ns", "dev-ns", "internal-services"];
const UPSTREAM_PORTS: &[u16] = &[80, 8080, 9000, 50051];
const AUTHORITIES: &[&str] = &[
    "api.example.com",
    "shop.example.org",
    "internal.svc.local",
    "data-pipeline.internal",
];
const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
    "curl/7.68.0",
    "Python-urllib/3.9",
    "Java/11.0.12 HttpClient",
    "kube-probe/1.23",
    "Prometheus/2.30.0",
];
const RESPONSE_FLAGS: &[&str] = &[
    "-", "UH", "UF", "UO", "NR", "DC", "LH", "UT", "LR", "URX", "NC", "DI", "FI", "RL",
];
const TLS_VERSIONS: &[&str] = &["TLSv1.2", "TLSv1.3"];
const TLS_CIPHERS: &[&str] = &[
    "AES128-GCM-SHA256",
    "AES256-GCM-SHA384",
    "CHACHA20-POLY1305-SHA256",
];

fn generate_pod_ip(rng: &mut StdRng) -> String {
    format!(
        "10.42.{}.{}",
        rng.random_range(0..256),
        rng.random_range(1..255)
    )
}

fn generate_external_ip(rng: &mut StdRng) -> String {
    format!(
        "{}.{}.{}.{}",
        rng.random_range(1..255),
        rng.random_range(0..256),
        rng.random_range(0..256),
        rng.random_range(1..255)
    )
}

pub fn generate_k8s_mesh_logs(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    let mut current_time = Utc::now() - Duration::days(rng.random_range(1..5));

    let status_code_choices: Vec<u16> = STATUS_CODES_WEIGHTED
        .iter()
        .flat_map(|&(code, weight)| std::iter::repeat(code).take(weight))
        .collect();

    for _ in 0..count {
        current_time += Duration::milliseconds(rng.random_range(50..5000));
        let timestamp = current_time.to_rfc3339_opts(SecondsFormat::Millis, true);
        let method = HTTP_METHODS.choose(&mut rng).unwrap_or(&"GET").to_string();

        let mut path_str = format!(
            "{}{}",
            COMMON_PATHS_PREFIX.choose(&mut rng).unwrap_or(&"/api/v1"),
            COMMON_PATHS_RESOURCE.choose(&mut rng).unwrap_or(&"items")
        );
        path_str.push_str(COMMON_PATHS_SUFFIX.choose(&mut rng).unwrap_or(&""));
        if path_str.contains(":id") {
            path_str = path_str.replace(":id", &rng.random_range(1..10000).to_string());
        }
        if rng.random_bool(0.4) {
            let num_params = rng.random_range(1..4);
            path_str.push('?');
            for i in 0..num_params {
                path_str.push_str(QUERY_PARAMS_KEYS.choose(&mut rng).unwrap_or(&"param"));
                path_str.push('=');
                path_str.push_str(&rng.random_range(1..1000).to_string());
                if i < num_params - 1 {
                    path_str.push('&');
                }
            }
        }

        let protocol = PROTOCOLS
            .choose(&mut rng)
            .unwrap_or(&"HTTP/1.1")
            .to_string();
        let status_code = *status_code_choices.choose(&mut rng).unwrap_or(&200);

        let bytes_sent = if method == "GET" && status_code == 200 {
            rng.random_range(100..50000)
        } else {
            rng.random_range(50..1000)
        };
        let bytes_received = if method == "POST" || method == "PUT" {
            rng.random_range(100..10000)
        } else {
            rng.random_range(50..500)
        };

        let mut duration_ms = rng.random_range(10..500);
        if status_code >= 500 {
            duration_ms += rng.random_range(100..1000);
        } else if status_code >= 400 {
            duration_ms += rng.random_range(20..200);
        } else if method == "POST" {
            duration_ms += rng.random_range(50..300);
        }

        let upstream_service_name = UPSTREAM_SERVICE_NAMES
            .choose(&mut rng)
            .unwrap_or(&"unknown-service")
            .to_string();
        let upstream_service_namespace = UPSTREAM_SERVICE_NAMESPACES
            .choose(&mut rng)
            .unwrap_or(&"default-ns")
            .to_string();
        let upstream_port = *UPSTREAM_PORTS.choose(&mut rng).unwrap_or(&8080);
        let upstream_cluster = format!(
            "outbound|{}||{}.{}.svc.cluster.local",
            upstream_port, upstream_service_name, upstream_service_namespace
        );
        let upstream_host_pod_ip = format!("{}:{}", generate_pod_ip(&mut rng), upstream_port);

        let downstream_is_pod = rng.random_bool(0.8);
        let downstream_remote_address = if downstream_is_pod {
            format!(
                "{}:{}",
                generate_pod_ip(&mut rng),
                rng.random_range(30000..60000)
            )
        } else {
            format!(
                "{}:{}",
                generate_external_ip(&mut rng),
                rng.random_range(10000..60000)
            )
        };
        let downstream_local_address_pod_ip = format!(
            "{}:{}",
            generate_pod_ip(&mut rng),
            *UPSTREAM_PORTS.choose(&mut rng).unwrap_or(&80)
        );

        let authority = AUTHORITIES
            .choose(&mut rng)
            .unwrap_or(&"default.example.com")
            .to_string();
        let user_agent = USER_AGENTS
            .choose(&mut rng)
            .unwrap_or(&"Unknown")
            .to_string();

        let x_forwarded_for = if !downstream_is_pod && rng.random_bool(0.7) {
            Some(
                downstream_remote_address
                    .split(':')
                    .next()
                    .unwrap_or("")
                    .to_string(),
            )
        } else if downstream_is_pod && rng.random_bool(0.2) {
            Some(format!(
                "{}, {}",
                generate_external_ip(&mut rng),
                generate_pod_ip(&mut rng)
            ))
        } else {
            None
        };

        let response_flags = RESPONSE_FLAGS.choose(&mut rng).unwrap_or(&"-").to_string();

        let (tls_version, tls_cipher) = if rng.random_bool(0.9) {
            (
                Some(
                    TLS_VERSIONS
                        .choose(&mut rng)
                        .unwrap_or(&"TLSv1.2")
                        .to_string(),
                ),
                Some(
                    TLS_CIPHERS
                        .choose(&mut rng)
                        .unwrap_or(&"AES128-GCM-SHA256")
                        .to_string(),
                ),
            )
        } else {
            (None, None)
        };

        let level = match status_code {
            s if s >= 500 => "error".to_string(),
            s if s >= 400 => "warn".to_string(),
            _ => "info".to_string(),
        };

        let entry = K8sMeshLogEntry {
            timestamp,
            level,
            request_id: Uuid::new_v4().to_string(),
            method,
            path: path_str,
            protocol,
            status_code,
            bytes_sent,
            bytes_received,
            duration_ms,
            upstream_service_name,
            upstream_service_namespace,
            upstream_cluster,
            upstream_host_pod_ip,
            downstream_remote_address,
            downstream_local_address_pod_ip,
            authority,
            user_agent,
            x_forwarded_for,
            response_flags,
            tls_version,
            tls_cipher,
        };

        logs.push(
            serde_json::to_string(&entry)
                .unwrap_or_else(|e| format!(r#"{{"error":"serialization failed: {}"}}"#, e)),
        );
    }
    logs
}
