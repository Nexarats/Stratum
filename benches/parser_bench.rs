//! Parser benchmarks.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn parser_benchmark(_c: &mut Criterion) {
    // TODO: Add parser benchmarks once the parser is stable.
    // Benchmark throughput: bytes per second through the ANSI parser.
}

criterion_group!(benches, parser_benchmark);
criterion_main!(benches);
