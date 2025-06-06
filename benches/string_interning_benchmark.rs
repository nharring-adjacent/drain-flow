use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use drain_flow::intern_benchmark_harness::{
    BucketBackendInterner, BufferBackendInterner, NoInterningBaseline, // SharedStringInterner,
    StringBackendInterner, StringInternerTrait,
};
// SharedStringInterner import commented out
use lazy_static::lazy_static; // Add this
use rand::rngs::StdRng; // Add this for a deterministic RNG
use rand::{Rng, SeedableRng}; // Add rand for data generation
use regex::Regex; // Add this
use std::collections::HashSet; // For Utc::now()

/*
Benchmark Notes for String Interning Strategies (Typical Results for this Workload):
- All interning strategies significantly outperform the `NoInterningBaseline` (String::clone).
- The `SharedStringInterner` (using the global BucketBackend) generally shows the best performance,
  likely benefiting from being a warm, shared instance.
- Among fresh interner instances:
    - `BufferBackendInterner` tends to be the fastest.
    - `StringBackendInterner` is slightly slower than BufferBackend.
    - `BucketBackendInterner` is typically the slowest of the interning backends in this test,
      though still much faster than no interning. Its specific strengths (e.g., 'static string
      handling) are not the primary focus of this dynamic tokenization benchmark.
- Performance variations are expected between runs due to system noise. The relative
  rankings are the most important takeaway.
*/

fn get_log_lines() -> Vec<String> {
    let mut lines = Vec::new();
    let mut rng = StdRng::seed_from_u64(42); // Use a fixed seed for reproducibility

    let users = ["alice", "bob", "charlie", "dave", "eve", "mallory"];
    let actions = [
        "logged_in",
        "logged_out",
        "viewed_page",
        "updated_profile",
        "posted_comment",
        "sent_message",
    ];
    let resources = [
        "/home",
        "/profile",
        "/settings",
        "/feed",
        "/messages",
        "/admin/users",
    ];
    let ip_prefixes = ["192.168.1", "10.0.0", "172.16.0", "203.0.113"];
    let error_messages = [
        "Failed to connect to database",
        "NullPointerException",
        "Disk space low",
        "Invalid credentials",
        "Request timed out",
        "Resource not found",
    ];
    let log_levels = ["INFO", "WARN", "ERROR", "DEBUG"];
    let common_words = [
        "the",
        "is",
        "a",
        "of",
        "in",
        "to",
        "from",
        "user",
        "service",
        "request",
        "response",
        "failed",
        "successful",
    ];

    for i in 0..10000 {
        // Generate 10,000 log lines
        let user = users[rng.random_range(0..users.len())];
        let action = actions[rng.random_range(0..actions.len())];
        let resource = resources[rng.random_range(0..resources.len())];
        let ip_suffix = rng.random_range(1..255);
        let ip_prefix = ip_prefixes[rng.random_range(0..ip_prefixes.len())];
        let status = if rng.random_bool(0.8) { "200" } else { "500" };
        let duration = rng.random_range(10..500);
        let level = log_levels[rng.random_range(0..log_levels.len())];
        let common1 = common_words[rng.random_range(0..common_words.len())];
        let common2 = common_words[rng.random_range(0..common_words.len())];

        let line = match i % 5 {
            0 => format!(
                "{} [{}]: User '{}' {} resource '{}' from {}.{}. Status: {}, Duration: {}ms. {} {}",
                level,
                chrono::Utc::now().to_rfc3339(),
                user,
                action,
                resource,
                ip_prefix,
                ip_suffix,
                status,
                duration,
                common1,
                common2
            ),
            1 => format!(
                "{} [{}]: {} - {} for user '{}'. Attempt from {}.{}. {} {}",
                level,
                chrono::Utc::now().to_rfc3339(),
                error_messages[rng.random_range(0..error_messages.len())],
                action,
                user,
                ip_prefix,
                ip_suffix,
                common1,
                common2
            ),
            2 => format!(
                "{} [{}]: Service health check: {}. Status: {}. {} {}",
                level,
                chrono::Utc::now().to_rfc3339(),
                common1,
                if rng.random_bool(0.9) {
                    "OK"
                } else {
                    "DEGRADED"
                },
                common1,
                common2
            ),
            3 => format!(
                "{} [{}]: {} {} {} {} {} {}",
                level,
                chrono::Utc::now().to_rfc3339(),
                common_words[rng.random_range(0..common_words.len())],
                common_words[rng.random_range(0..common_words.len())],
                common_words[rng.random_range(0..common_words.len())],
                common_words[rng.random_range(0..common_words.len())],
                common_words[rng.random_range(0..common_words.len())],
                common_words[rng.random_range(0..common_words.len())]
            ),
            _ => format!(
                "{} [{}]: User '{}' performed action '{}'. Details: {} {} {}. IP: {}.{}",
                level,
                chrono::Utc::now().to_rfc3339(),
                user,
                action,
                common1,
                common2,
                resource,
                ip_prefix,
                ip_suffix
            ),
        };
        lines.push(line);
    }
    lines
}

lazy_static! {
    static ref TOKEN_RE: Regex = Regex::new(
        r#"(?x)
        (\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}) | # IP Addresses
        ([0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}) | # UUIDs
        (\d+\.\d+|\d+) | # Numbers (float or int)
        ([=():\[\]{}<>]) | # Delimiters
        ([\w-]+) | # Words (alphanumeric, hyphen, underscore)
        (\S) # Any other non-whitespace character
    "#
    )
    .unwrap();
}

fn tokenize_line(line: &str) -> Vec<&str> {
    TOKEN_RE.find_iter(line).map(|mat| mat.as_str()).collect()
}

fn intern_lines<T: StringInternerTrait>(interner: &mut T, lines: &[String]) -> HashSet<T::Symbol> {
    let mut unique_symbols = HashSet::new();
    for line in lines {
        let tokens = tokenize_line(line);
        for token in tokens {
            unique_symbols.insert(interner.intern(token));
        }
    }
    unique_symbols
}

fn string_interning_benchmark(c: &mut Criterion) {
    let log_lines = get_log_lines();
    // Calculate total number of bytes for throughput calculation
    let total_bytes = log_lines.iter().map(|s| s.len()).sum::<usize>();

    let mut group = c.benchmark_group("StringInterningStrategies");
    group.throughput(Throughput::Bytes(total_bytes as u64));

    /*
    // SharedStringInterner benchmark commented out
    group.bench_function(
        BenchmarkId::new("SharedStringInterner", "string-interner"),
        |b| {
            b.iter_with_setup(
                || (SharedStringInterner::new(), log_lines.clone()), // Setup: create interner and clone lines
                |(mut interner, lines)| intern_lines(&mut interner, &lines), // Action: intern the lines
            );
        },
    );
    */

    group.bench_function(
        BenchmarkId::new("NoInterningBaseline", "String::clone"),
        |b| {
            b.iter_with_setup(
                || (NoInterningBaseline::new(), log_lines.clone()),
                |(mut interner, lines)| intern_lines(&mut interner, &lines),
            );
        },
    );

    group.bench_function(
        BenchmarkId::new("StringBackendInterner", "fresh-string-backend"),
        |b| {
            b.iter_with_setup(
                || (StringBackendInterner::new(), log_lines.clone()),
                |(mut interner, lines)| intern_lines(&mut interner, &lines),
            );
        },
    );

    group.bench_function(
        BenchmarkId::new("BucketBackendInterner", "bucket-backend"),
        |b| {
            b.iter_with_setup(
                || (BucketBackendInterner::new(), log_lines.clone()),
                |(mut interner, lines)| intern_lines(&mut interner, &lines),
            );
        },
    );

    group.bench_function(
        BenchmarkId::new("BufferBackendInterner", "buffer-backend"),
        |b| {
            b.iter_with_setup(
                || (BufferBackendInterner::new(), log_lines.clone()),
                |(mut interner, lines)| intern_lines(&mut interner, &lines),
            );
        },
    );

    group.finish();
}

criterion_group!(benches, string_interning_benchmark);
criterion_main!(benches);
