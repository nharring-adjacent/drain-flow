extern crate serde_derive;
extern crate tinytemplate;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use drain_flow::log_group::LogGroup;
use drain_flow::record::Record;
use drain_flow::SimpleDrain;

// Really simplistic benchmark of adding new lines using a constant line
// this is pretty unrealistic since after the first one the string interner
// will be doing almost no work but its a start
pub fn benchmark_new_lines(c: &mut Criterion) {
    let mut drain = SimpleDrain::new(vec![]).unwrap();
    c.bench_function("new_lines", |b| {
        b.iter(|| {
            drain.process_line(black_box(
                "Sample line with a few words to score".to_string(),
            ))
        })
    });
}

pub fn benchmark_calculate_score(c: &mut Criterion) {
    let rec1 = Record::new("Sample line with a few words to score".to_string());
    let rec2 = Record::new("Different log line which will not match".to_string());
    c.bench_function("calculate_simscore", |b| {
        b.iter(|| {
            rec1.calc_sim_score(black_box(&rec2));
        });
    });
}

pub fn benchmark_find_variables(c: &mut Criterion) {
    let rec1 = Record::new("Sample line with a few words to score: 12345".to_string());
    let rec2 = Record::new("Sample line with a few words to score: 65432123".to_string());

    let lg = LogGroup::new(rec1);
    c.bench_function("find_variables", |b| {
        b.iter(|| {
            lg.discover_variables(black_box(&rec2)).unwrap();
        });
    });
}

pub fn benchmark_add_example(c: &mut Criterion) {
    let rec1 = Record::new("Sample line with a few words to score: 12345".to_string());
    let rec2 = Record::new("Sample line with a few words to score: 65432123".to_string());
    let mut lg = LogGroup::new(rec1);

    c.bench_function("add_example", |b| {
        b.iter(|| {
            lg.add_example(rec2.clone());
        });
    });
}

criterion_group!(
    benches,
    benchmark_new_lines,
    benchmark_calculate_score,
    benchmark_find_variables,
    benchmark_add_example
);
criterion_main!(benches);
