use chrono::{DateTime, Utc};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use drain_flow::log_group::LogGroup;
use drain_flow::record::Record;
use drain_flow::SimpleDrain;
use generators::{RecordTemplate, Sendmail};
use rand::prelude::*;

use rand::Rng;

mod generators;
use self::generators::LogGenerator;

pub fn benchmark_sink(c: &mut Criterion) {
    let mut drain = SimpleDrain::new(vec![]).unwrap();
    let generator = LogGenerator::new().unwrap();
    let mut rng = rand::thread_rng();
    for size in [100usize, 500usize, 1000usize, 5000usize] {
        let lines = (0..size)
            .into_iter()
            .map(|_| {
                generator.make_record(RecordTemplate::Sendmail(Sendmail {
                    ts: Utc::now().to_string(),
                    remote: format!(
                        "{}.{}.{}.{}",
                        rng.gen_range(1..255),
                        rng.gen_range(1..255),
                        rng.gen_range(1..255),
                        rng.gen_range(1..255)
                    ),
                    status: 300usize,
                    message: "baz".to_string(),
                }))
            })
            .collect::<Vec<String>>();
        let mut group = c.benchmark_group("sink many lines");
        group.throughput(Throughput::Elements(lines.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("process ", lines.len()),
            &lines,
            |b, lines| {
                b.iter(|| {
                    for l in lines {
                        drain.process_line(l.to_string()).unwrap();
                    }
                });
            },
        );
        group.finish();
    }
}

criterion_group!(sink, benchmark_sink,);

criterion_main!(sink);
