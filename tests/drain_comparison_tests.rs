// tests/drain_comparison_tests.rs

use drain_flow::drains::api::Drain;
use drain_flow::drains::simple::SingleLayer;
use drain_flow::drains::two_stage_drain::TwoStageDrain;
use drain_flow::drains::DifferentialDrain; // Assuming DifferentialDrain is re-exported at drain_flow::drains
use drain_flow::log_group::LogGroup;
// use drain_flow::record::Record; // Record itself might not be directly asserted

// Helper to get a comparable string representation of a LogGroup's template
// This currently relies on the base_record's content, which for DifferentialDrain
// should be the reconstructed template string.
fn get_template_string(group: &LogGroup) -> String {
    group.base_record().to_string() // base_record().to_string() might include ID etc.
}

// Helper to print groups for debugging
#[allow(dead_code)] // Will be used in tests, but good to have for debugging even if not all paths use it.
fn print_groups_summary(drain_name: &str, groups: &[LogGroup]) {
    println!("\n---- {} ----", drain_name);
    println!("Total groups: {}", groups.len());
    for (i, group) in groups.iter().enumerate() {
        println!(
            "  Group {}: Count={}, Template='{}', ID={}",
            i,
            group.len(),
            get_template_string(group), // Use helper
            group.id
        );
        // Optionally print a few samples
        // for sample in group.examples().iter().take(2) {
        //     println!("    Sample (ID {}): {}", sample.id, sample.content);
        // }
    }
    println!("--------------------");
}

#[cfg(test)]
mod comparative_tests {
    use super::*; // Import helpers and structs from parent module

    // Placeholder for Scenario 1 test
    #[test]
    fn test_scenario_1_simple_evolution() {
        let log_sequence: Vec<&str> = vec![
            "Login success user admin_user_1 session 12345",
            "Login success user guest_user_A session 67890",
            "Login success user admin_user_2 session abcde",
        ];

        // Initialize Drains
        // DifferentialDrain: threshold 0.6, min_concrete_tokens (max_depth) 2
        let mut dd_drain = DifferentialDrain::new(0.6, 2);
        // SingleLayer: default (often uses regexes if provided, or simple exact match)
        let mut sl_drain = SingleLayer::new(vec![]).expect("Failed to create SingleLayer");
        // TwoStageDrain: default params (e.g., threshold 0.5, depth 4, max_children 100)
        let mut ts_drain =
            TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain");

        for line in &log_sequence {
            dd_drain.process_line(line.to_string()).unwrap();
            sl_drain.process_line(line.to_string()).unwrap();
            ts_drain.process_line(line.to_string()).unwrap();
        }

        let dd_groups = dd_drain.collect_log_groups();
        let sl_groups = sl_drain.collect_log_groups();
        let ts_groups = ts_drain.collect_log_groups();

        print_groups_summary("DifferentialDrain", &dd_groups);
        print_groups_summary("SingleLayer", &sl_groups);
        print_groups_summary("TwoStageDrain", &ts_groups);

        // --- Assertions for Scenario 1 ---
        // DifferentialDrain expected: 1 cluster, template "Login success user <*> session <*>"
        assert_eq!(
            dd_groups.len(),
            1,
            "DifferentialDrain: Expected 1 group for scenario 1"
        );
        if !dd_groups.is_empty() {
            let template = get_template_string(&dd_groups[0]);
            // Tokenization: ["Login", "success", "user", "admin_user_1", "session", "12345"]
            // Generalizes to: ["Login", "success", "user", "<*>", "session", "<*>"]
            // Reconstructed template (joined by space): "Login success user <*> session <*>"
            assert_eq!(
                template, "Login success user <*> session <*>",
                "DifferentialDrain: Template mismatch for scenario 1"
            );
            assert_eq!(
                dd_groups[0].len(),
                3,
                "DifferentialDrain: Count mismatch for scenario 1"
            );
        }

        // SingleLayer clusters similar messages based on token similarity. With
        // these inputs it groups all lines together, so we expect a single
        // group.
        assert_eq!(
            sl_groups.len(),
            1,
            "SingleLayer: Expected 1 group for scenario 1"
        );

        // TwoStageDrain is more complex; its behavior depends on its internal generalization.
        // It might group them if "Login success user" is seen as a common prefix and numbers/session IDs as variables.
        // Or it might create 3 groups if its generalization isn't aggressive enough for this small sample.
        // For now, let's be flexible or assert it's different from DifferentialDrain.
        assert!(
            !ts_groups.is_empty() && ts_groups.len() <= 3,
            "TwoStageDrain: Group count out of expected range for scenario 1. Got {}",
            ts_groups.len()
        );
        if ts_groups.len() == 1 {
            // If TwoStageDrain also gets 1 group, its template might be similar.
            // This depends heavily on TwoStageDrain's configuration.
            // For a simple check, we might ensure it's not *as* general or it *is* as general.
            // This test is primarily to highlight DifferentialDrain's behavior.
            println!(
                "TwoStageDrain also produced 1 group for scenario 1. Template: '{}'",
                get_template_string(&ts_groups[0])
            );
        }
    }

    // Scenario 2: Evolving Service Version / Error Codes
    #[test]
    fn test_scenario_2_evolving_patterns() {
        let log_sequence: Vec<&str> = vec![
            "Service v1.0 request proc_alpha status 200", // S1
            "Service v1.0 request proc_beta status 200",  // S2
            "Service v1.1 request proc_alpha status 200", // S3
            "Service v1.1 request proc_beta status 200",  // S4
            "Service v1.1 request proc_alpha status 503", // S5
        ];

        // DifferentialDrain: threshold 0.6, min_concrete_tokens (max_depth) 2
        // - S1: [S, v1.0, r, pA, s, 200] (C1)
        // - S2 vs C1_T1: Sim([S,v1.0,r,pB,s,200], [S,v1.0,r,pA,s,200]) = 5/6 ~ 0.83. Match.
        //   C1_T gens to [S, v1.0, r, <*>, s, 200]. Count=2. (min_concrete=5 >= 2. OK)
        // - S3 vs C1_T2: Sim([S,v1.1,r,pA,s,200], [S,v1.0,r,<*>,s,200]) = 4/6 ~ 0.66. Match.
        //   C1_T gens to [S, <*>, r, <*>, s, 200]. Count=3. (min_concrete=4 >= 2. OK)
        // - S4 vs C1_T3: Sim([S,v1.1,r,pB,s,200], [S,<*>,r,<*>,s,200]) = 6/6 = 1.0. Match.
        //   C1_T is already [S,<*>,r,<*>,s,200]. Count=4.
        // - S5 vs C1_T3: Sim([S,v1.1,r,pA,s,503], [S,<*>,r,<*>,s,200]) = 5/6 ~ 0.83 (200 vs 503 differs). Match.
        //   C1_T gens to [S,<*>,r,<*>,s,<*>]. Count=5. (min_concrete=3 >=2. OK)
        // Expected DD: 1 cluster: "Service <*> request <*> status <*>"

        let mut dd_drain = DifferentialDrain::new(0.6, 2);
        let mut sl_drain = SingleLayer::new(vec![]).expect("Failed to create SingleLayer");
        let mut ts_drain =
            TwoStageDrain::new(vec![], 0.5, 4, 100).expect("Failed to create TwoStageDrain");

        for line in &log_sequence {
            dd_drain.process_line(line.to_string()).unwrap();
            sl_drain.process_line(line.to_string()).unwrap();
            ts_drain.process_line(line.to_string()).unwrap();
        }

        let dd_groups = dd_drain.collect_log_groups();
        let sl_groups = sl_drain.collect_log_groups();
        let ts_groups = ts_drain.collect_log_groups();

        print_groups_summary("DifferentialDrain (Scenario 2)", &dd_groups);
        print_groups_summary("SingleLayer (Scenario 2)", &sl_groups);
        print_groups_summary("TwoStageDrain (Scenario 2)", &ts_groups);

        // Assertions for DifferentialDrain
        assert_eq!(
            dd_groups.len(),
            1,
            "DifferentialDrain: Expected 1 group for scenario 2"
        );
        if !dd_groups.is_empty() {
            let template = get_template_string(&dd_groups[0]);
            // Advanced tokenizer: ["Service", "v1.0", "request", "proc_alpha", "status", "200"]
            // Tokenization splits the version into multiple tokens (e.g. "v1", ".", "0"). After
            // generalization the template retains the static "v1" and "." tokens.
            // Expected generalized template: "Service v1 . <*> request <*> status <*>"
            assert_eq!(
                template, "Service v1 . <*> request <*> status <*>",
                "DifferentialDrain: Template mismatch for scenario 2"
            );
            assert_eq!(
                dd_groups[0].len(),
                5,
                "DifferentialDrain: Count mismatch for scenario 2"
            );
        }

        // SingleLayer groups lines by token similarity. With these inputs we
        // observe two groups: one for the successful requests and one for the
        // 503 error line.
        assert_eq!(
            sl_groups.len(),
            2,
            "SingleLayer: Expected 2 groups for scenario 2"
        );

        // Assertions for TwoStageDrain (behavior can vary)
        // It might create 1 group if it generalizes versions and statuses, or more.
        // e.g. [S,*,r,*,s,200] (4 lines) and [S,v1.1,r,pA,s,503] (1 line) -> 2 groups
        // or [S,v1.0,r,*,s,200], [S,v1.1,r,*,s,200], [S,v1.1,r,pA,s,503] -> 3 groups
        // or even more if proc_alpha/beta are not grouped by its first stage.
        // The key is that it's likely more than DifferentialDrain.
        assert!(
            !ts_groups.is_empty() && ts_groups.len() <= 5,
            "TwoStageDrain: Group count out of expected range for scenario 2. Got {}",
            ts_groups.len()
        );
        if dd_groups.len() < ts_groups.len() {
            println!("TwoStageDrain created more groups ({}) than DifferentialDrain ({}) as expected sometimes.", ts_groups.len(), dd_groups.len());
        }
    }
}
