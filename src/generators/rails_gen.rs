//! Generates realistic Ruby on Rails application log entries, simulating request lifecycles.
// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use std::fmt;
use rand::{Rng, SeedableRng, rngs::StdRng};
// Alphanumeric import removed
use chrono::{Utc, Duration, SecondsFormat};
use uuid::Uuid;

// 1. Define helper enums/structs

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    INFO,
    WARN,
    ERROR,
    DEBUG,
    FATAL,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::INFO => write!(f, "INFO"),
            Severity::WARN => write!(f, "WARN"),
            Severity::ERROR => write!(f, "ERROR"),
            Severity::DEBUG => write!(f, "DEBUG"),
            Severity::FATAL => write!(f, "FATAL"),
        }
    }
}

impl Severity {
    fn first_letter(&self) -> char {
        match self {
            Severity::INFO => 'I',
            Severity::WARN => 'W',
            Severity::ERROR => 'E',
            Severity::DEBUG => 'D',
            Severity::FATAL => 'F',
        }
    }
}

#[derive(Debug)]
pub struct HttpRequestContext {
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub ip_address: String,
    pub controller_action: String,
    pub params: Option<String>,
    pub pid: u32,
}

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH"];
const PATHS: &[&str] = &[
    "/users", "/users/new", "/users/123", "/users/123/edit",
    "/products", "/products/search", "/products/789",
    "/orders", "/orders/mine", "/orders/456/details",
    "/api/v1/items", "/api/v1/items/1",
];
const CONTROLLERS: &[&str] = &["UsersController", "ProductsController", "OrdersController", "Api::V1::ItemsController"];
const ACTIONS: &[&str] = &["index", "show", "create", "update", "destroy", "search", "new", "edit"];
const PARAM_KEYS: &[&str] = &["page", "search_term", "category_id", "user_id", "product_id", "utf8", "authenticity_token"];
const TEMPLATES: &[&str] = &["index.html.erb", "show.html.erb", "form.html.erb", "_item.html.erb", "results.json.jbuilder"];
const DB_ACTIONS: &[&str] = &["User Load", "Product Load", "Order Load", "Session Save", "Cache Read", "Cache Write"];


fn format_log_line(
    severity: Severity,
    timestamp: &chrono::DateTime<Utc>,
    pid: u32,
    request_id: &str,
    message: &str,
) -> String {
    format!(
        "{}, [{} #{}] {} -- : [REQUEST_ID: {}] {}", // Removed space before #PID
        severity.first_letter(),
        timestamp.to_rfc3339_opts(SecondsFormat::Micros, true),
        pid,
        severity,
        request_id,
        message
    )
}

pub fn generate_rails_request_logs(seed: u64) -> Vec<String> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut logs = Vec::new();
    // Each request should have its own timestamp sequence, so initialize current_time here
    let mut current_time = Utc::now() - Duration::seconds(rng.gen_range(0..3600)); // Start some time in the past

    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ\
                            abcdefghijklmnopqrstuvwxyz\
                            0123456789";

    let request_id: String = Uuid::new_v4().to_string().chars().take(12).collect();
    let method = HTTP_METHODS.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "GET".to_string());
    let path = PATHS.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "/users".to_string());
    let ip_address = format!("192.168.{}.{}", rng.gen_range(0..256), rng.gen_range(1..255));
    let controller = CONTROLLERS.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "UsersController".to_string());
    let action = ACTIONS.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "index".to_string());
    let controller_action = format!("{}#{}", controller, action);
    let pid = rng.gen_range(10000..30000);

    let params_present = rng.gen_bool(0.7); // 70% chance of having params
    let params = if params_present {
        let num_params = rng.gen_range(1..4);
        let mut param_map = String::from("{");
        for i in 0..num_params {
            let param_key = PARAM_KEYS.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "default_param".to_string());
            let val_len = rng.gen_range(5..10);
            let param_value: String = (0..val_len)
                .map(|_| {
                    let idx = rng.gen_range(0..CHARSET.len());
                    CHARSET[idx] as char
                })
                .collect();
            param_map.push_str(&format!("\"{}\"=>\"{}\"", param_key, param_value));
            if i < num_params - 1 {
                param_map.push_str(", ");
            }
        }
        param_map.push_str("}");
        Some(param_map)
    } else {
        None
    };

    let context = HttpRequestContext {
        request_id: request_id.clone(),
        method: method.clone(),
        path: path.clone(),
        ip_address: ip_address.clone(),
        controller_action: controller_action.clone(),
        params: params.clone(),
        pid,
    };

    current_time += Duration::microseconds(rng.gen_range(0..100_000)); // Add initial jitter to start time
    logs.push(format_log_line(
        Severity::INFO,
        &current_time,
        context.pid,
        &context.request_id,
        &format!("Started {} \"{}\" for {}", context.method, context.path, context.ip_address),
    ));

    current_time += Duration::microseconds(rng.gen_range(100..5000));
    logs.push(format_log_line(
        Severity::INFO,
        &current_time,
        context.pid,
        &context.request_id,
        &format!("Processing by {} as HTML", context.controller_action),
    ));

    if let Some(p_str) = &context.params {
        current_time += Duration::microseconds(rng.gen_range(50..2000));
        logs.push(format_log_line(
            Severity::INFO,
            &current_time,
            context.pid,
            &context.request_id,
            &format!("Parameters: {}", p_str),
        ));
    }

    let is_error_request = rng.gen_bool(0.15);

    if !is_error_request {
        let num_db_queries = rng.gen_range(1..=3);
        for _ in 0..num_db_queries {
            current_time += Duration::microseconds(rng.gen_range(200..10_000)); // DB queries can take longer
            let db_action = DB_ACTIONS.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "User Load".to_string());
            let db_time_ms = rng.gen_range(0.1..15.0) as f32; // Increased upper bound for DB time
            let query_example = match db_action {
                "User Load" => "SELECT \"users\".* FROM \"users\" WHERE \"users\".\"id\" = ? LIMIT ?",
                "Product Load" => "SELECT \"products\".* FROM \"products\" WHERE \"products\".\"slug\" = ? LIMIT ?",
                "Order Load" => "SELECT \"orders\".* FROM \"orders\" WHERE \"orders\".\"user_id\" = ? ORDER BY \"orders\".\"created_at\" DESC",
                _ => "SELECT pg_sleep(0.001);" // More plausible short query
            };
            logs.push(format_log_line(
                Severity::DEBUG,
                &current_time,
                context.pid,
                &context.request_id,
                &format!("{} ({:.1}ms)  {}", db_action, db_time_ms, query_example),
            ));
        }

        current_time += Duration::microseconds(rng.gen_range(1000..20_000)); // View rendering
        let template = TEMPLATES.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "index.html.erb".to_string());
        let view_time_ms = rng.gen_range(5.0..150.0) as f32; // Increased view time
        logs.push(format_log_line(
            Severity::INFO,
            &current_time,
            context.pid,
            &context.request_id,
            &format!("Rendered {} (Duration: {:.1}ms | Allocations: {})", template, view_time_ms, rng.gen_range(1000..20000)),
        ));

        current_time += Duration::microseconds(rng.gen_range(100..5000));
        let total_duration_ms = rng.gen_range(10.0..500.0) as f32; // Increased total duration
        let active_record_ms = rng.gen_range(1.0..(total_duration_ms * 0.6).max(1.1)) as f32; // ActiveRecord can be a larger portion
        let allocations = rng.gen_range(5000..50000);
        logs.push(format_log_line(
            Severity::INFO,
            &current_time,
            context.pid,
            &context.request_id,
            &format!(
                "Completed 200 OK in {:.1}ms (Views: {:.1}ms | ActiveRecord: {:.1}ms | Allocations: {})", // Removed overall "Duration" as it's often the sum
                view_time_ms + active_record_ms + rng.gen_range(1.0..5.0), // Simplified total, ensuring it's > sum of parts
                view_time_ms,
                active_record_ms,
                allocations
            ),
        ));
    } else {
        current_time += Duration::microseconds(rng.gen_range(500..10_000));
        logs.push(format_log_line(
            Severity::ERROR,
            &current_time,
            context.pid,
            &context.request_id,
            "Something went wrong processing the request!",
        ));

        let error_descriptions = [
            "NoMethodError: undefined method `foo' for nil:NilClass",
            "ActiveRecord::RecordNotFound: Couldn't find User with 'id'=nonexistent",
            "ActionController::RoutingError: No route matches [GET] \"/nonexistent_path\"",
            "RuntimeError: A critical service failed to respond",
            "ArgumentError: wrong number of arguments (given 1, expected 0)",
            "TypeError: no implicit conversion of String into Integer"
        ];
        let backtrace_files = [
            "app/controllers/users_controller.rb", "app/models/product.rb", "app/services/payment_service.rb",
            "gems/activerecord-7.0.3/lib/active_record/core.rb", "gems/actionpack-7.0.3/lib/action_controller/metal/strong_parameters.rb"
        ];
        let backtrace_methods = [
            "block in index", "process_payment", "find_by_sql", "handle_unverified_request", "new", "create"
        ];


        let chosen_error = error_descriptions.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "NoMethodError: undefined method `foo' for nil:NilClass".to_string());
        current_time += Duration::microseconds(rng.gen_range(100..3000));
        logs.push(format_log_line(
            Severity::FATAL,
            &current_time,
            context.pid,
            &context.request_id,
            &format!("{}", chosen_error), // Removed "Error:" prefix as it's part of the description
        ));

        let num_backtrace_lines = rng.gen_range(2..=4);
        for _ in 0..num_backtrace_lines {
            current_time += Duration::microseconds(rng.gen_range(50..1500));
            let file = backtrace_files.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "app/controllers/application_controller.rb".to_string());
            let line_num = rng.gen_range(10..200);
            let method = backtrace_methods.choose(&mut rng).map(|s| s.to_string()).unwrap_or_else(|| "unknown_method".to_string());
            logs.push(format_log_line(
                Severity::FATAL, // Backtrace lines are typically FATAL in this context
                &current_time,
                context.pid,
                &context.request_id,
                &format!("  {}:{}:in `{}'", file, line_num, method),
            ));
        }
    }

    logs
}

pub fn generate_rails_app_logs(num_requests: usize, seed: u64) -> Vec<String> {
    let mut main_rng = StdRng::seed_from_u64(seed);
    let mut all_logs = Vec::new();

    for _ in 0..num_requests {
        let request_seed = main_rng.gen::<u64>();
        let mut request_logs = generate_rails_request_logs(request_seed);
        all_logs.append(&mut request_logs);
    }

    // Sort logs by timestamp to simulate a single application's log file
    // This requires parsing the timestamp from each log line, which is complex here.
    // For now, we'll assume that the sequential generation within generate_rails_request_logs
    // and the loop order is sufficient for a reasonably ordered log.
    // A more robust solution would parse and sort.

    all_logs
}
