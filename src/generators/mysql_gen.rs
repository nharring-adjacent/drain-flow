//! Generates realistic MySQL slow query log entries.
// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use chrono::{Duration, Utc}; // DateTime removed
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
// Removed SampleRange import as Rng should provide random_range and random_bool

const USERS: &[&str] = &["root", "app_user", "backup_user", "etl_process"];
const HOSTS: &[&str] = &["db_primary.example.com", "db_replica_1.example.com", "analytics_db.example.com"];
const CLIENT_IPS: &[Option<&str>] = &[Some("10.0.1.23"), Some("192.168.1.105"), Some("172.16.3.41"), None];
const QUERIES: &[&str] = &[
    "SELECT * FROM users WHERE id = ?;",
    "SELECT name, email FROM customers WHERE last_login > ? AND status = 'active';",
    "UPDATE products SET price = price * ? WHERE category = ?;",
    "INSERT INTO orders (customer_id, product_id, quantity, order_date) VALUES (?, ?, ?, ?);",
    "SELECT o.order_id, c.customer_name, p.product_name, oi.quantity FROM orders o JOIN customers c ON o.customer_id = c.customer_id JOIN order_items oi ON o.order_id = oi.order_id JOIN products p ON oi.product_id = p.product_id WHERE o.order_date BETWEEN ? AND ?;",
    "SELECT COUNT(*), status FROM tasks GROUP BY status HAVING COUNT(*) > ?;",
    "DELETE FROM event_logs WHERE event_timestamp < ?;",
];


#[derive(Debug)]
pub struct MySqlSlowQueryLogEntry {
    pub timestamp: String, // e.g., "2023-10-27T10:20:45.987654Z"
    pub user: String,      // e.g., "app_user"
    pub host: String,      // e.g., "db_server.example.com"
    pub client_ip: Option<String>, // e.g., Some("10.0.1.23")
    pub connection_id: String, // e.g., "89"
    pub query_time: f64,   // e.g., 5.123456
    pub lock_time: f64,    // e.g., 0.000050
    pub rows_sent: usize,
    pub rows_examined: usize,
    pub unix_timestamp: u64, // e.g., 1698397245
    pub query: String,     // the actual SQL query
}

pub fn format_log_entry(entry: &MySqlSlowQueryLogEntry) -> String {
    let client_ip_str = entry.client_ip.as_deref().unwrap_or("");
    format!(
        "# Time: {}\n# User@Host: {}[{}] @ {} [{}] Id: {}\n# Query_time: {:.6} Lock_time: {:.6} Rows_sent: {} Rows_examined: {}\nSET timestamp={};\n{};",
        entry.timestamp,
        entry.user,
        entry.user, 
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
    let mut current_time = Utc::now();

    for i in 0..count {
        let user_index = rng.gen_range(0..USERS.len());
        let user = USERS[user_index].to_string();
        let host_index = rng.gen_range(0..HOSTS.len());
        let host = HOSTS[host_index].to_string();
        let client_ip_index = rng.gen_range(0..CLIENT_IPS.len());
        let client_ip = CLIENT_IPS[client_ip_index].map(String::from);
        let query_index = rng.gen_range(0..QUERIES.len());
        let query = QUERIES[query_index].to_string();
        
        // Make query time somewhat correlated with query complexity (longer queries take more time)
        let query_complexity_factor = query.len();
        let base_query_time: f64 = rng.gen_range(0.1..2.0) + (query_complexity_factor as f64 / 100.0);
        let is_slow_query: bool = rng.gen_bool(0.2);
        let query_time = if is_slow_query {
            base_query_time * rng.gen_range(5.0..20.0) // Significantly longer for slow queries
        } else {
            base_query_time
        };

        let lock_time = rng.gen_range(0.00001..0.5) * query_time; // Lock time is a fraction of query time
        let rows_sent = rng.gen_range(0..(100 + (query_complexity_factor * 2)));
        let rows_examined = rng.gen_range(rows_sent..(5000 + (query_complexity_factor * 10)));
        
        // Increment timestamp slightly for each log
        current_time += Duration::milliseconds(rng.gen_range(50..5000));
        let timestamp_str = current_time.to_rfc3339_opts(chrono::SecondsFormat::Micros, true);


        let entry = MySqlSlowQueryLogEntry {
            timestamp: timestamp_str,
            user,
            host,
            client_ip,
            connection_id: (1000 + i).to_string(), // Simple incrementing connection ID
            query_time,
            lock_time,
            rows_sent,
            rows_examined,
            unix_timestamp: current_time.timestamp() as u64,
            query,
        };
        logs.push(format_log_entry(&entry));
    }

    logs
}
