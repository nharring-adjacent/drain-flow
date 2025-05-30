// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

//! Generates realistic Ruby on Rails application log entries, simulating request lifecycles.

use chrono::{Duration, SecondsFormat, Utc};
use rand::prelude::IndexedRandom;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy)]
enum Severity {
    DEBUG,
    INFO,
    WARN,
    ERROR,
    FATAL,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Severity {
    fn first_letter(&self) -> char {
        match self {
            Severity::DEBUG => 'D',
            Severity::INFO => 'I',
            Severity::WARN => 'W',
            Severity::ERROR => 'E',
            Severity::FATAL => 'F',
        }
    }
}

struct HttpRequestContext {
    request_id: String,
    method: String,
    path: String,
    ip_address: String,
    controller_action: String,
    params_string: Option<String>,
    pid: u32,
}

const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "DELETE", "PATCH"];
const PATHS: &[&str] = &[
    "/users",
    "/users/:id",
    "/products",
    "/products/:id/details",
    "/orders",
    "/cart",
    "/admin/dashboard",
];
const CONTROLLERS: &[&str] = &[
    "UsersController",
    "ProductsController",
    "OrdersController",
    "Admin::DashboardsController",
];
const ACTIONS: &[&str] = &[
    "index", "show", "create", "update", "destroy", "edit", "new",
];
pub(crate) const PARAM_KEYS: &[&str] = &[
    "page",
    "per_page",
    "sort_by",
    "filter",
    "id",
    "product_id",
    "user_id",
    "utf8",
    "authenticity_token",
];
const DB_ACTIONS: &[&str] = &[
    "User Load",
    "Product Load",
    "Order Update",
    "Session Create",
    "Cache Read",
];
const TEMPLATES: &[&str] = &[
    "users/index.html.erb",
    "products/show.html.erb",
    "layouts/application.html.erb",
];
const ERROR_DESCRIPTIONS: &[&str] = &[
    "NoMethodError: undefined method `foo' for nil:NilClass",
    "ActiveRecord::RecordNotFound: Couldn't find User with 'id'=XXX",
    "ActionController::RoutingError: No route matches [GET] \"/nonexistent_path\"",
    "RuntimeError: Something unexpected went wrong",
    "ArgumentError: wrong number of arguments (given X, expected Y)",
    "TypeError: no implicit conversion of nil into String",
];
const BACKTRACE_FILES: &[&str] = &[
    "app/controllers/users_controller.rb",
    "app/models/product.rb",
    "lib/custom_middleware.rb",
    "gems/activerecord-x.y.z/lib/active_record/base.rb",
    "app/services/payment_processor.rb",
];
const BACKTRACE_METHODS: &[&str] = &[
    "block (2 levels) in create",
    "process_payment",
    "find_by_id",
    "validate_user_input",
    "render_template",
    "handle_exception",
];
const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

fn generate_rails_request_logs(
    rng: &mut StdRng,
    base_time: &mut chrono::DateTime<Utc>,
) -> Vec<String> {
    let mut logs = Vec::new();

    let request_id: String = Uuid::new_v4().to_string().chars().take(12).collect();
    let method = HTTP_METHODS
        .choose(rng)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "GET".to_string());
    let path_template = PATHS
        .choose(rng)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "/users".to_string());
    let path = path_template.replace(":id", &rng.random_range(1..1000).to_string());
    let ip_address = format!(
        "192.168.{}.{}",
        rng.random_range(0..256),
        rng.random_range(1..255)
    );
    let controller = CONTROLLERS
        .choose(rng)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "UsersController".to_string());
    let action = ACTIONS
        .choose(rng)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "index".to_string());
    let pid = rng.random_range(10000..30000);

    let params_present = rng.random_bool(0.7);
    let params_string = if params_present {
        let num_params = rng.random_range(1..4);
        let mut params_map_str = String::from("{");
        for i in 0..num_params {
            let param_key = PARAM_KEYS
                .choose(rng)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "default_param".to_string());

            let val_len = rng.random_range(5..10);
            let param_value: String = (0..val_len)
                .map(|_| {
                    let idx = rng.random_range(0..CHARSET.len());
                    CHARSET[idx] as char
                })
                .collect();

            params_map_str.push_str(&format!("\"{}\"=>\"{}\"", param_key, param_value));
            if i < num_params - 1 {
                params_map_str.push_str(", ");
            }
        }
        params_map_str.push('}');
        Some(params_map_str)
    } else {
        None
    };

    let context = HttpRequestContext {
        request_id,
        method,
        path,
        ip_address,
        controller_action: format!("{}#{}", controller, action),
        params_string,
        pid,
    };

    let initial_jitter_micros = rng.random_range(0..100_000);
    *base_time += Duration::microseconds(initial_jitter_micros);

    logs.push(format_log_line(
        *base_time,
        Severity::INFO,
        &context.request_id,
        context.pid,
        &format!(
            "Started {} \"{}\" for {} at {}",
            context.method,
            context.path,
            context.ip_address,
            base_time.format("%Y-%m-%d %H:%M:%S %z")
        ),
    ));

    *base_time += Duration::microseconds(rng.random_range(100..5000));
    logs.push(format_log_line(
        *base_time,
        Severity::INFO,
        &context.request_id,
        context.pid,
        &format!("Processing by {} as HTML", context.controller_action),
    ));

    if let Some(ref params) = context.params_string {
        *base_time += Duration::microseconds(rng.random_range(50..2000));
        logs.push(format_log_line(
            *base_time,
            Severity::INFO,
            &context.request_id,
            context.pid,
            &format!("  Parameters: {}", params),
        ));
    }

    let is_error_request = rng.random_bool(0.15);

    if !is_error_request {
        let num_db_queries = rng.random_range(1..=3);
        for _ in 0..num_db_queries {
            *base_time += Duration::microseconds(rng.random_range(200..10_000));
            let db_action = DB_ACTIONS
                .choose(rng)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "User Load".to_string());
            let db_time_ms = rng.random_range(0.1..15.0) as f32;
            logs.push(format_log_line(
                *base_time,
                Severity::DEBUG,
                &context.request_id,
                context.pid,
                &format!(
                    "{} ({:.1}ms) SELECT \"users\".* FROM \"users\" WHERE id = {}",
                    db_action,
                    db_time_ms,
                    rng.random_range(1..1000)
                ),
            ));
        }

        *base_time += Duration::microseconds(rng.random_range(1000..20_000));
        let template = TEMPLATES
            .choose(rng)
            .map(|s| s.to_string())
            .unwrap_or_else(|| "index.html.erb".to_string());
        let view_time_ms = rng.random_range(5.0..150.0) as f32;
        let active_record_total_ms: f32 = rng.random_range(0.5..20.0);

        logs.push(format_log_line(
            *base_time,
            Severity::INFO,
            &context.request_id,
            context.pid,
            &format!(
                "Rendered {} (Duration: {:.1}ms | Views: {:.1}ms | ActiveRecord: {:.1}ms)",
                template,
                view_time_ms + active_record_total_ms,
                view_time_ms,
                active_record_total_ms
            ),
        ));

        *base_time += Duration::microseconds(rng.random_range(100..5000));
        let total_duration_ms = rng.random_range(10.0..500.0) as f32;
        let active_record_ms = rng.random_range(1.0..(total_duration_ms * 0.6).max(1.1)) as f32;
        let allocations = rng.random_range(5000..50000);
        logs.push(format_log_line(*base_time, Severity::INFO, &context.request_id, context.pid,
            &format!("Completed 200 OK in {:.0}ms (Views: {:.1}ms | ActiveRecord: {:.1}ms | Allocations: {})", 
                view_time_ms + active_record_ms + rng.random_range(1.0..5.0),
                view_time_ms,
                active_record_ms,
                allocations
            )
        ));
    } else {
        *base_time += Duration::microseconds(rng.random_range(500..10_000));
        logs.push(format_log_line(
            *base_time,
            Severity::ERROR,
            &context.request_id,
            context.pid,
            "Something went wrong processing the request!",
        ));

        let chosen_error = ERROR_DESCRIPTIONS
            .choose(rng)
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                "NoMethodError: undefined method `foo' for nil:NilClass".to_string()
            });
        *base_time += Duration::microseconds(rng.random_range(100..3000));
        logs.push(format_log_line(
            *base_time,
            Severity::FATAL,
            &context.request_id,
            context.pid,
            &format!("  Error: {}", chosen_error),
        ));

        let num_backtrace_lines = rng.random_range(2..=4);
        for _ in 0..num_backtrace_lines {
            *base_time += Duration::microseconds(rng.random_range(50..1500));
            let file = BACKTRACE_FILES
                .choose(rng)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "app/controllers/application_controller.rb".to_string());
            let line_num = rng.random_range(10..200);
            let method_name = BACKTRACE_METHODS
                .choose(rng)
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unknown_method".to_string());
            logs.push(format_log_line(
                *base_time,
                Severity::FATAL,
                &context.request_id,
                context.pid,
                &format!("    {}:{}:in `{}'", file, line_num, method_name),
            ));
        }
    }
    logs
}

fn format_log_line(
    timestamp: chrono::DateTime<Utc>,
    severity: Severity,
    request_id: &str,
    pid: u32,
    message: &str,
) -> String {
    format!(
        "{}, [{}Z #{}] {} -- [REQUEST_ID: {}] {}",
        severity.first_letter(),
        timestamp.to_rfc3339_opts(SecondsFormat::Micros, false),
        pid,
        severity,
        request_id,
        message
    )
}

pub fn generate_rails_app_logs(num_requests: usize, seed: u64) -> Vec<String> {
    let mut main_rng = StdRng::seed_from_u64(seed);
    let mut all_logs = Vec::new();
    let mut current_base_time = Utc::now() - Duration::days(main_rng.random_range(1..10));

    for _ in 0..num_requests {
        let request_seed = main_rng.random::<u64>();
        let mut request_rng = StdRng::seed_from_u64(request_seed);
        let request_logs = generate_rails_request_logs(&mut request_rng, &mut current_base_time);
        all_logs.extend(request_logs);
        current_base_time += Duration::seconds(main_rng.random_range(0..5));
    }
    all_logs
}
