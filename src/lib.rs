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

pub mod drains;
pub mod log_group;
pub mod record;

/// # Log Querying
///
/// This module provides structures and functions for querying processed log data that has
/// been structured into `LogGroup`s. The primary structure for accessing and managing
/// this data is the `LogStore`.
///
/// Key functionalities include:
/// - Storing and retrieving `LogGroup`s by their unique ID.
/// - Filtering `LogGroup`s based on a specific time range.
/// - Performing range-based aggregation on the example records of a specific `LogGroup`
///   (identified by its ID) via the `query::query_log_range_aggregation` function.
///   This allows for detailed analysis of log events matching a particular pattern
///   within a given time window.
pub mod query;
