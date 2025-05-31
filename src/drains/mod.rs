//! # Log Processing Drains
//!
//! This module provides mechanisms for processing, clustering, and managing log data.
//! It includes the core `Drain` trait (defined in [`api::Drain`]), which establishes
//! a common interface for various log processing strategies. Different implementations
//! of this trait, such as [`simple::SingleLayer`] and [`two_stage_drain::TwoStageDrain`],
//! offer distinct approaches to log analysis.
//!
//! The primary purpose of this module is to abstract the specifics of log processing,
//! allowing other parts of the system to interact with log data through a consistent API.

// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

pub mod api;
pub mod simple;
pub mod two_stage_drain;
pub mod differential_drain;
pub use differential_drain::DifferentialDrain;

