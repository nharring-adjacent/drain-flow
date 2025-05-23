use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};
use drain_flow::drains::{DifferentialDrain, SimpleDrain};
use timely::Config;
// Timestamps in DifferentialDrain are now u64, not RootTimestamp for the main logic.
// Tests might use RootTimestamp, but the drain itself expects W::Timestamp = u64.
// So, benchmarks should use u64.

// Helper function to generate log lines
fn generate_log_lines(count: usize, cardinality: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..count {
        lines.push(format!("Log message template_{} unique_val_{}", i % cardinality, i));
    }
    lines
}

fn benchmark_differential_drain(c: &mut Criterion) {
    let mut group = c.benchmark_group("DifferentialDrain");

    // Define scenarios: (name, line_count, cardinality)
    let scenarios = [
        ("1k_lines_low_card", 1_000, 10),
        ("10k_lines_low_card", 10_000, 10),
        ("1k_lines_high_card", 1_000, 500),
        ("10k_lines_high_card", 10_000, 5_000),
        ("100k_lines_low_card", 100_000, 100),
        ("100k_lines_high_card", 100_000, 10_000),
    ];

    for (id, line_count, cardinality) in scenarios.iter() {
        let log_lines = generate_log_lines(*line_count, *cardinality);
        
        // DifferentialDrain needs to run within a timely dataflow computation.
        // We are benchmarking the act of processing these lines and flushing.
        group.bench_function(*id, |b| {
            b.iter_batched(
                || log_lines.clone(), // Setup: clone data for each iteration
                |lines| { // Routine: the code to benchmark
                    // Using timely::execute::execute instead of execute_from_args for cleaner setup in benches
                    timely::execute::execute(Config::thread(), move |worker| {
                        let mut drain = DifferentialDrain::new(worker);
                        for (idx, line) in lines.into_iter().enumerate() {
                            // Timestamps must advance.
                            drain.process_line(black_box(line), RootTimestamp::new(idx as u64));
                        }
                        // Ensure all data is processed by advancing time and flushing.
                        drain.flush(RootTimestamp::new(lines.len() as u64));
                        
                        // Step the worker until outstanding work related to these inputs is done.
                        // This is a simplified way to ensure processing for benchmarking purposes.
                        // A more robust way might involve probes if we were checking output,
                        // but here we focus on input processing throughput.
                        
                        // Step until input timestamps are processed
                        while worker.progress().map_or(true, |p| p.0 <= lines.len() as u64) {
                            worker.step();
                        }
                        // Additional steps to help settle feedback loops from re-evaluation
                        // and ensure quiescence.
                        // The number of steps here is heuristic. A more advanced setup might
                        // involve custom signals or probing internal collections if possible in bench.
                        for _ in 0..(worker.peers() * 2 + 15) { // Increased steps slightly
                            // Check if the worker thinks it's done or progress has advanced sufficiently.
                            if worker.outstanding_work().map_or(false, |o| o == 0) &&
                               worker.progress().map_or(false, |p| p.0 > lines.len() as u64) {
                                break; 
                            }
                            worker.step();
                        }
                        // Final check for any remaining outstanding work.
                        while worker.outstanding_work().map_or(true, |o| o > 0) {
                             worker.step();
                        }
                    }).unwrap();
                },
                BatchSize::SmallInput, // Adjust if setup cost is too high or iterations too short
            );
        });
    }
    group.finish();
}

fn benchmark_simple_drain(c: &mut Criterion) {
    let mut group = c.benchmark_group("SimpleDrain");

    let scenarios = [
        ("1k_lines_low_card", 1_000, 10),
        ("10k_lines_low_card", 10_000, 10),
        ("1k_lines_high_card", 1_000, 500),
        ("10k_lines_high_card", 10_000, 5_000),
        ("100k_lines_low_card", 100_000, 100),
        ("100k_lines_high_card", 100_000, 10_000),
    ];

    for (id, line_count, cardinality) in scenarios.iter() {
        let log_lines = generate_log_lines(*line_count, *cardinality);
        
        group.bench_function(*id, |b| {
            b.iter_batched(
                || {
                    // Setup: clone data and create drain for each iteration batch
                    (log_lines.clone(), SimpleDrain::new(vec![]).unwrap())
                },
                |(lines, mut drain)| { // Routine: the code to benchmark
                    for line in lines {
                        drain.process_line(black_box(line)).unwrap();
                    }
                },
                BatchSize::SmallInput, 
            );
        });
    }
    group.finish();
}

criterion_group!(benches, benchmark_differential_drain, benchmark_simple_drain);
criterion_main!(benches);
