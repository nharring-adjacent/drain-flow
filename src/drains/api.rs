// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use crate::log_group::LogGroup;
use anyhow::Error;

/// Defines the core interface for log processing drains.
///
/// The `Drain` trait abstracts the mechanism by which log lines are processed,
/// clustered into `LogGroup`s, and subsequently retrieved. Implementations of
/// this trait can offer various strategies for log analysis, such as simple
/// in-memory storage or more complex, multi-stage processing pipelines.
///
/// This abstraction allows other parts of the system, like the `LogStore`,
/// to operate on log data generically, without being coupled to a specific
/// drain implementation.
pub trait Drain {
    /// Processes a single log line, potentially updating internal log group structures.
    ///
    /// # Parameters
    ///
    /// * `line`: A `String` representing the log line to be processed.
    ///
    /// # Returns
    ///
    /// * `Ok(true)`: If processing the line resulted in the creation of a new `LogGroup`.
    /// * `Ok(false)`: If the line was successfully processed and added to an existing `LogGroup`.
    /// * `Err(anyhow::Error)`: If an error occurred during processing.
    fn process_line(&mut self, line: String) -> Result<bool, Error>;

    /// Retrieves all unique `LogGroup`s currently managed by the drain.
    ///
    /// This method provides a snapshot of the log groups at the time of calling.
    /// The order of log groups in the returned vector is not guaranteed.
    ///
    /// # Returns
    ///
    /// A `Vec<LogGroup>` containing all log groups. If no log lines have been
    /// processed or no groups have been formed, an empty vector is returned.
    fn collect_log_groups(&self) -> Vec<LogGroup>;
}
