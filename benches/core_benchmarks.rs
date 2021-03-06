// Copyright Nicholas Harring. All rights reserved.
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the Server Side Public License, version 1, as published by MongoDB, Inc.
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the Server Side Public License for more details. You should have received a copy of the
// Server Side Public License along with this program.
// If not, see <http://www.mongodb.com/licensing/server-side-public-license>.

extern crate serde_derive;
extern crate tinytemplate;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use drain_flow::{drains::simple::SingleLayer, log_group::LogGroup, record::Record};

// Really simplistic benchmark of adding new lines using a constant line
// this is pretty unrealistic since after the first one the string interner
// will be doing almost no work but its a start
pub fn benchmark_new_lines(c: &mut Criterion) {
    let mut drain = SingleLayer::new(vec![]).unwrap();
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
