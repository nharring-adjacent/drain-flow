// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

//! Generates realistic MySQL slow query log entries.

use chrono::{Duration, SecondsFormat, Utc};
use rand::{rngs::StdRng, Rng, SeedableRng};
// Removed: use rand::seq::SliceRandom; // Not used in this version

const USERS: &[&str] = &["root", "app_user", "backup_user", "reporting_user"];
const HOSTS: &[&str] = &[
    "localhost",
    "db_server.example.com",
    "10.0.1.23",
    "web_app_server",
];
const CLIENT_IPS: &[&str] = &["192.168.1.100", "10.0.5.12", "172.16.30.5", ""]; // Empty string for no IP
const QUERIES: &[(&str, usize)] = &[
    // (Query, ComplexityFactor: higher means more complex/slower)
    ("SELECT * FROM users WHERE id = ?;", 50),
    (
        "SELECT * FROM products WHERE category = ? ORDER BY price DESC;",
        100,
    ),
    ("UPDATE orders SET status = ? WHERE id = ?;", 80),
    ("INSERT INTO logs (level, message) VALUES (?, ?);", 30),
    (
        "SELECT COUNT(*) FROM large_table WHERE created_at > ? AND status = ?;",
        200,
    ),
    ("DELETE FROM sessions WHERE last_seen < ?;", 60),
    ("CALL process_daily_report(?);", 300),
    ("SELECT @@version_comment LIMIT 1;", 5),
    ("SHOW STATUS LIKE 'Uptime';", 10),
];

#[derive(Debug)]
pub struct MySqlSlowQueryLogEntry {
    pub timestamp: String,
    pub user: String,
    pub host: String,
    pub client_ip: Option<String>,
    pub connection_id: String,
    pub query_time: f64,
    pub lock_time: f64,
    pub rows_sent: usize,
    pub rows_examined: usize,
    pub unix_timestamp: u64,
    pub query: String,
}

pub fn format_log_entry(entry: &MySqlSlowQueryLogEntry) -> String {
    let client_ip_str = entry.client_ip.as_deref().unwrap_or("");
    format!(
        "# Time: {}
# User@Host: {}[{}] @ {} [{}] Id: {}
# Query_time: {:.6} Lock_time: {:.6} Rows_sent: {} Rows_examined: {}
SET timestamp={};
{};",
        entry.timestamp,
        entry.user,
        entry.user, // User often repeated in brackets
        entry.host,
        client_ip_str,
        entry.connection_id,
        entry.query_time,
        entry.lock_time,
        entry.rows_sent,
        entry.rows_examined,
        entry.unix_timestamp,
        entry.query
    )
}

pub fn generate_mysql_slow_query_logs(count: usize, seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::with_capacity(count);
    let mut current_time = Utc::now() - Duration::days(rng.random_range(1..30)); // Start some time in the past

    for i in 0..count {
        let user_index = rng.random_range(0..USERS.len());
        let user = USERS[user_index].to_string();
        let host_index = rng.random_range(0..HOSTS.len());
        let host = HOSTS[host_index].to_string();

        let client_ip_index = rng.random_range(0..CLIENT_IPS.len());
        let client_ip_str = CLIENT_IPS[client_ip_index];
        let client_ip = if client_ip_str.is_empty() {
            None
        } else {
            Some(client_ip_str.to_string())
        };

        let query_index = rng.random_range(0..QUERIES.len());
        let (query_template, query_complexity_factor) = QUERIES[query_index];

        // Simulate query parameters (simple replacement for now)
        let query =
            query_template.replace("?", &format!("'param_val_{}'", rng.random_range(1..1000)));

        let base_query_time: f64 =
            rng.random_range(0.1..2.0) + (query_complexity_factor as f64 / 100.0);
        let is_slow_query: bool = rng.random_bool(0.2); // 20% chance of being a "slow" query beyond base time

        let query_time = if is_slow_query {
            base_query_time * rng.random_range(5.0..20.0) // Significantly longer for slow queries
        } else {
            base_query_time
        };

        let lock_time = rng.random_range(0.00001..0.5) * query_time; // Lock time is a fraction of query time
        let rows_sent = rng.random_range(0..(100 + (query_complexity_factor * 2)));
        let rows_examined = rng.random_range(rows_sent..(5000 + (query_complexity_factor * 10)));
        let connection_id = (1000 + i).to_string(); // Simple sequential ID

        current_time += Duration::milliseconds(rng.random_range(50..5000));
        let timestamp_str = current_time.to_rfc3339_opts(SecondsFormat::Micros, true);
        let unix_timestamp_val = current_time.timestamp() as u64;

        let entry = MySqlSlowQueryLogEntry {
            timestamp: timestamp_str,
            user,
            host,
            client_ip,
            connection_id,
            query_time,
            lock_time,
            rows_sent,
            rows_examined,
            unix_timestamp: unix_timestamp_val,
            query,
        };
        logs.push(format_log_entry(&entry));
    }
    logs
}
