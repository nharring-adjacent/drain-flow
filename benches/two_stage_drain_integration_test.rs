// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

use anyhow::Result;
use drain_flow::drains::{api::Drain, two_stage_drain::TwoStageDrain}; // Adjust path if necessary, Added api::Drain

#[allow(dead_code)] // Suppress warning, as this is used by a test
fn básico_drain_test_harness(lines: Vec<String>, expected_groups: usize) -> Result<()> {
    let mut drain = TwoStageDrain::new(vec![], 0.5, 4, 10)?; // Using default values for now

    for line in lines {
        Drain::process_line(&mut drain, line)?; // Updated call
    }

    let groups = drain.collect_log_groups();
    assert_eq!(
        groups.len(),
        expected_groups,
        "Mismatch in expected number of log groups. Processed {} lines.",
        drain.line_count_processed
    );

    println!(
        "Processed {} lines. Verified {} groups.",
        drain.line_count_processed,
        groups.len()
    );

    Ok(())
}

fn main() {
    // This benchmark is intended to be run with `cargo test --bench two_stage_drain_integration_test`
    // or by directly invoking test functions if used as a library.
    // Adding a dummy main for `harness = false` when `cargo check --benches` is run.
    println!("Run tests in this file using `cargo test --benches` or specific test invocation.");
}

#[test]
fn test_basic_drain_integration_simple_lines() -> Result<()> {
    let lines = vec![
        "Log message type A value1".to_string(),
        "Log message type A value2".to_string(), // Should match first
        "Completely different log message valueX".to_string(), // New group
        "Log message type A value3".to_string(), // Should match first
        "Another different message type Y".to_string(), // New group
    ];
    // Expected groups:
    // 1. "Log message type A <*>"
    // 2. "Completely different log message <*>"
    // 3. "Another different message type <*>"
    básico_drain_test_harness(lines, 3)?;
    Ok(())
}

#[test]
fn test_basic_drain_integration_with_preprocessing() -> Result<()> {
    let mut drain = TwoStageDrain::new(
        vec![
            r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z".to_string(), // ISO8601 Timestamp
            r"user-\d+".to_string(),                             // User ID
        ],
        0.6, // Slightly higher threshold
        5,   // Max depth
        100, // Max children (less restrictive for this test)
    )?;

    let lines = vec![
        "2023-10-27T10:00:00Z user-123 System started successfully".to_string(),
        "2023-10-27T10:01:15Z user-456 System started successfully".to_string(), // Match 1 (vars: date, user)
        "2023-10-27T10:02:30Z user-123 System shutdown initiated".to_string(),   // New group
        "2023-10-27T10:03:00Z user-789 System started successfully".to_string(), // Match 1
        "2023-10-27T10:04:00Z user-456 System shutdown initiated".to_string(),   // Match 2
        "2023-10-27T10:05:00Z user-123 System started successfully".to_string(), // Match 1
    ];
    let line_count = lines.len(); // Store line count before moving lines

    for line in lines {
        Drain::process_line(&mut drain, line)?; // Updated call
    }

    // Expected groups:
    // 1. "<*> <*> System started successfully"
    // 2. "<*> <*> System shutdown initiated"
    let groups = drain.collect_log_groups();
    assert_eq!(
        groups.len(),
        2, // Expected number of groups after preprocessing
        "Mismatch in expected number of log groups after preprocessing. Processed {} lines.",
        line_count // Use stored line_count
    );
    println!(
        "Processed {} lines with preprocessing. Verified {} groups.",
        line_count, // Use stored line_count
        groups.len()
    );
    Ok(())
}
