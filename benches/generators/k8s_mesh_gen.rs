// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use rand::{Rng, SeedableRng, seq::SliceRandom, rngs::StdRng};
use chrono::{Utc, DateTime, Duration, SecondsFormat};
use serde::Serialize;
use serde_json;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct K8sMeshLogEntry {
    pub timestamp: String, 
    pub level: String,     
    pub request_id: String, 
    pub method: String,    
    pub path: String,      
    pub protocol: String,  
    pub status_code: u16,  
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub duration_ms: u32,  
    pub upstream_service_name: String, 
    pub upstream_service_namespace: String, 
    pub upstream_cluster: String, 
    pub upstream_host_pod_ip: String, 
    pub downstream_remote_address: String, 
    pub downstream_local_address_pod_ip: String, 
    pub authority: String, 
    pub user_agent: String,
    pub x_forwarded_for: Option<String>,
    pub response_flags: String, 
    pub tls_version: Option<String>, 
    pub tls_cipher: Option<String>, 
}

const LEVELS: &[&str] = &["info", "warn", "error"]; // "debug" is often too verbose for access logs
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"];
const HTTP_PROTOCOLS: &[&str] = &["HTTP/1.1", "HTTP/2.0"];
const COMMON_PATHS_PREFIX: &[&str] = &["/api/v1", "/api/v2", "/app", "/ui", "/service"];
const COMMON_PATHS_RESOURCE: &[&str] = &[
    "users", "products", "orders", "items", "payments", "status", "health", "config", "search", "metrics"
];
const COMMON_PATHS_SUFFIX: &[&str] = &["", "/:id", "/:id/details", "/bulk", "/stream"];
const QUERY_PARAMS_KEYS: &[&str] = &["session_id", "user_id", "product_id", "category", "limit", "offset", "q", "filter", "sort_by"];

// Weighted status codes: more 2xx, some 4xx, fewer 5xx/3xx
const STATUS_CODES: &[(u16, usize)] = &[
    (200, 50), (201, 10), (204, 5), // OK, Created, No Content
    (301, 3), (302, 3), (304, 4),   // Redirects, Not Modified
    (400, 7), (401, 5), (403, 5), (404, 5), (429, 2), // Client Errors
    (500, 3), (502, 1), (503, 1), (504, 1), // Server Errors
];

const UPSTREAM_SERVICE_NAMES: &[&str] = &[
    "product-catalog", "user-authentication", "order-processing", "payment-gateway", "inventory-management", "recommendation-engine", "notification-service"
];
const UPSTREAM_SERVICE_NAMESPACES: &[&str] = &["prod-ns", "staging-ns", "dev-ns", "infra-ns"];
const UPSTREAM_PORTS: &[u16] = &[80, 8080, 9000, 5000, 3000];

const USER_AGENTS: &[&str] = &[
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/100.0.4896.127 Safari/537.36",
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.0 Safari/605.1.15",
    "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.0 Mobile/15E148 Safari/604.1",
    "curl/7.79.1",
    "PostmanRuntime/7.29.0",
    "python-requests/2.27.1",
    "kube-probe/1.23",
    "gRPC-Java/1.45.0",
    "Prometheus/2.34.0",
    "Go-http-client/1.1",
];
const RESPONSE_FLAGS: &[&str] = &["-", "UH", "UF", "UO", "URX", "NC", "LH", "UT", "LR", "DC"];
const TLS_VERSIONS: &[&str] = &["TLSv1.2", "TLSv1.3"];
const TLS_CIPHERS: &[&str] = &[
    "AEAD-AES128-GCM-SHA256", "AEAD-AES256-GCM-SHA384", "AEAD-CHACHA20-POLY1305-SHA256",
    "ECDHE-RSA-AES128-GCM-SHA256", "ECDHE-RSA-AES256-GCM-SHA384", "ECDHE-ECDSA-AES128-GCM-SHA256",
];
const AUTHORITIES: &[&str] = &[
    "shop.example.com", "api.example.com", "internal-service.corp", "my-app.prod-ns.svc.cluster.local", "prometheus-operator.monitoring.svc"
];

fn generate_pod_ip(rng: &mut StdRng) -> String {
    format!("10.42.{}.{}", rng.gen_range(0..256), rng.gen_range(1..255))
}
fn generate_external_ip(rng: &mut StdRng) -> String {
    format!("{}.{}.{}.{}", rng.gen_range(1..255), rng.gen_range(0..256), rng.gen_range(0..256), rng.gen_range(1..255))
}


pub fn generate_k8s_mesh_logs(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    let mut current_time = Utc::now() - Duration::days(rng.gen_range(1..5)); // Start up to 5 days ago

    let status_code_choices: Vec<u16> = STATUS_CODES
        .iter()
        .flat_map(|&(val, weight)| std::iter::repeat(val).take(weight))
        .collect();

    for _i in 0..count {
        current_time += Duration::milliseconds(rng.gen_range(50..5000));

        let method = HTTP_METHODS.choose(&mut rng).unwrap_or(&"GET").to_string();
        let mut path_str = format!(
            "{}/{}{}",
            COMMON_PATHS_PREFIX.choose(&mut rng).unwrap_or(&"/api/v1"),
            COMMON_PATHS_RESOURCE.choose(&mut rng).unwrap_or(&"items"),
            COMMON_PATHS_SUFFIX.choose(&mut rng).unwrap_or(&"")
        );
        if path_str.contains(":id") {
            path_str = path_str.replace(":id", &rng.gen_range(1..10000).to_string());
        }
        if rng.gen_bool(0.4) { // 40% chance of query params
            let num_params = rng.gen_range(1..4);
            path_str.push('?');
            for j in 0..num_params {
                path_str.push_str(QUERY_PARAMS_KEYS.choose(&mut rng).unwrap_or(&"param"));
                path_str.push('=');
                path_str.push_str(&rng.gen_range(1..1000).to_string()); // Simple numeric values for params
                if j < num_params - 1 {
                    path_str.push('&');
                }
            }
        }

        let status_code = *status_code_choices.choose(&mut rng).unwrap_or(&200);
        
        let bytes_sent = if method == "GET" && status_code == 200 { rng.gen_range(100..50000) } else { rng.gen_range(50..1000) };
        let bytes_received = if method == "POST" || method == "PUT" { rng.gen_range(100..10000) } else { rng.gen_range(50..500) };
        
        let mut duration_ms = rng.gen_range(10..500); // Base duration
        if status_code >= 500 { duration_ms += rng.gen_range(100..1000); } // Slower for server errors
        else if status_code >= 400 { duration_ms += rng.gen_range(20..200); } // Slightly slower for client errors
        else if method == "POST" { duration_ms += rng.gen_range(50..300); } // POSTs might take longer

        let upstream_service_name = UPSTREAM_SERVICE_NAMES.choose(&mut rng).unwrap_or(&"unknown-service").to_string();
        let upstream_service_namespace = UPSTREAM_SERVICE_NAMESPACES.choose(&mut rng).unwrap_or(&"default-ns").to_string();
        let upstream_port = UPSTREAM_PORTS.choose(&mut rng).unwrap_or(&8080);
        let upstream_cluster = format!(
            "outbound|{}||{}.{}.svc.cluster.local",
            upstream_port, upstream_service_name, upstream_service_namespace
        );
        let upstream_host_pod_ip = format!("{}:{}", generate_pod_ip(&mut rng), upstream_port);
        
        let downstream_is_pod = rng.gen_bool(0.8); // 80% of traffic from other pods
        let downstream_remote_address = if downstream_is_pod {
            format!("{}:{}", generate_pod_ip(&mut rng), rng.gen_range(30000..60000))
        } else {
            format!("{}:{}", generate_external_ip(&mut rng), rng.gen_range(10000..60000))
        };
        let downstream_local_address_pod_ip = format!("{}:{}", generate_pod_ip(&mut rng), UPSTREAM_PORTS.choose(&mut rng).unwrap_or(&80));

        let authority = AUTHORITIES.choose(&mut rng).unwrap_or(&"default.example.com").to_string();
        let user_agent = USER_AGENTS.choose(&mut rng).unwrap_or(&"Unknown").to_string();
        
        let x_forwarded_for = if !downstream_is_pod && rng.gen_bool(0.7) { // 70% of external traffic has XFF
            Some(generate_external_ip(&mut rng))
        } else if downstream_is_pod && rng.gen_bool(0.2) { // 20% of internal traffic might have XFF (e.g. internal LB)
             Some(format!("{}, {}", generate_pod_ip(&mut rng), generate_pod_ip(&mut rng))) // chain of internal XFF
        }
        else {
            None
        };

        let response_flags = RESPONSE_FLAGS.choose(&mut rng).unwrap_or(&"-").to_string();

        let (tls_version, tls_cipher) = if rng.gen_bool(0.9) { // 90% of requests use TLS
            (
                Some(TLS_VERSIONS.choose(&mut rng).unwrap_or(&"TLSv1.2").to_string()),
                Some(TLS_CIPHERS.choose(&mut rng).unwrap_or(&"AES128-GCM-SHA256").to_string())
            )
        } else {
            (None, None)
        };
        
        let level = if status_code >= 500 { "error".to_string() } 
                    else if status_code >= 400 { "warn".to_string() } 
                    else { LEVELS.choose(&mut rng).unwrap_or(&"info").to_string() };


        let entry = K8sMeshLogEntry {
            timestamp: current_time.to_rfc3339_opts(SecondsFormat::Millis, true),
            level,
            request_id: Uuid::new_v4().to_string(),
            method,
            path: path_str,
            protocol: HTTP_PROTOCOLS.choose(&mut rng).unwrap_or(&"HTTP/1.1").to_string(),
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

        match serde_json::to_string(&entry) {
            Ok(json_string) => logs.push(json_string),
            Err(e) => eprintln!("Failed to serialize K8sMeshLogEntry: {}", e), // Should not happen with valid struct
        }
    }

    logs
}
