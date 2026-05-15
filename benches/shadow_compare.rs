//! Micro-bench for the divergence diff path. The diff is the cheap part — the
//! interesting number is "how much overhead does the shadower add on top of
//! the primary backend." Run with `cargo bench`.

use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use request_shadow::{Divergence, ResponseRecord, ShadowConfig};

fn bench_compare(c: &mut Criterion) {
    let primary = ResponseRecord::ok(vec![0u8; 4096])
        .with_header("content-type", "application/json")
        .with_header("x-trace-id", "abc123");
    let shadow = ResponseRecord::ok(vec![0u8; 4096])
        .with_header("content-type", "application/json")
        .with_header("x-trace-id", "abc124");
    let config = ShadowConfig::full_sample();

    c.bench_function("divergence_compare_4kb_equal_body", |b| {
        b.iter(|| {
            let _ = Divergence::compare(&primary, &shadow, &config);
        });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30).measurement_time(Duration::from_secs(3));
    targets = bench_compare
}
criterion_main!(benches);
