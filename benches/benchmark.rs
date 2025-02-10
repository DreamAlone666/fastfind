use criterion::{criterion_group, criterion_main, Criterion, SamplingMode, Throughput};
use std::time::Duration;

use ffd::{scan_drivers, Volume};

fn file_records_buf(c: &mut Criterion) {
    let drv = scan_drivers().into_iter().next().unwrap();
    let vol = Volume::open(drv).unwrap();

    let mut group = c.benchmark_group("file_records_buf");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    const N: usize = 10_0000;
    group.throughput(Throughput::Elements(N as _));

    const KB: usize = 1024;
    group.bench_function("4k", |b| {
        b.iter(|| for _ in vol.file_records::<{ 4 * KB }>().take(N) {})
    });
    group.bench_function("16k", |b| {
        b.iter(|| for _ in vol.file_records::<{ 16 * KB }>().take(N) {})
    });
    group.bench_function("64k", |b| {
        b.iter(|| for _ in vol.file_records::<{ 64 * KB }>().take(N) {})
    });

    group.finish();
}

criterion_group!(benches, file_records_buf);
criterion_main!(benches);
