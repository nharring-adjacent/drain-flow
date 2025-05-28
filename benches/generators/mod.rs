// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

// Module declarations for the new generators
pub mod k8s_infra_gen;
pub mod k8s_mesh_gen;
pub mod mysql_gen;
pub mod rails_gen;
pub mod syslog_gen;

// Re-export the main generation functions from each module
pub use k8s_infra_gen::generate_k8s_infra_logs;
pub use k8s_mesh_gen::generate_k8s_mesh_logs;
pub use mysql_gen::generate_mysql_slow_query_logs;
pub use rails_gen::generate_rails_app_logs;
pub use syslog_gen::generate_syslog_messages;

// Old code (RecordTemplate, Json, NGINXAccess, etc., LogGenerator) has been removed.
// Unused imports (HashMap, TinyTemplate, anyhow::Error, serde_derive) have been removed.
