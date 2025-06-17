// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

#[macro_use]
extern crate custom_derive;
#[macro_use]
extern crate enum_derive;

pub mod core_structures;
pub mod drain_parser;
pub mod drains;
pub mod intern_benchmark_harness;
pub mod log_group;
pub mod record;
pub mod runtime;

/// # Log Querying
///
/// This module provides structures and functions for querying processed log data.
/// Log data is structured into `LogGroup`s, which are collections of similar log records.
/// The primary structure for querying this data is the [`query::LogStore`].
///
/// The `LogStore` is generic and operates on data provided by a [`drains::api::Drain`]
/// implementation. This means it doesn't own the log data directly but rather queries
/// it from the underlying drain.
///
/// Key functionalities include:
/// - Retrieving `LogGroup`s by their unique ID via the `LogStore`.
/// - Filtering `LogGroup`s based on a specific time range using the `LogStore`.
/// - Executing LogQL queries against the `LogStore` using [`query::execute_logql_query`].
/// - Performing range-based aggregation on records (from specific log groups or LogQL queries)
///   within a given time window, facilitated by [`query::query_log_range_aggregation`].
pub mod query;
