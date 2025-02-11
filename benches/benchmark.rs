use criterion::{criterion_group, criterion_main, Criterion, SamplingMode, Throughput};
use std::{sync::Arc, thread::spawn, time::Duration};

use ffd::{scan_drivers, Volume};

const KB: usize = 1024;

fn file_records_iter<const BS: usize>(vol: &Volume, n: usize) {
    let mut count = 0;
    for res in vol.file_records::<BS>().take(n) {
        res.unwrap();
        count = count + 1;
    }
    assert_eq!(count, n);
}

fn file_records_buf(c: &mut Criterion) {
    let drv = scan_drivers().into_iter().next().unwrap();
    let vol = Volume::open(drv).unwrap();

    let mut group = c.benchmark_group("file_records_buf");
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    const N: usize = 10_0000;
    group.throughput(Throughput::Elements(N as _));

    group.bench_function("4k", |b| {
        b.iter(|| file_records_iter::<{ 4 * KB }>(&vol, N))
    });
    group.bench_function("16k", |b| {
        b.iter(|| file_records_iter::<{ 16 * KB }>(&vol, N))
    });
    group.bench_function("64k", |b| {
        b.iter(|| file_records_iter::<{ 64 * KB }>(&vol, N))
    });

    group.finish();
}

fn file_records_threading(c: &mut Criterion) {
    let vols: Vec<_> = scan_drivers()
        .into_iter()
        .map(|drv| Volume::open(drv).unwrap())
        .map(Arc::new)
        .collect();

    let group_name = format!("file_records_threading/{}vols", vols.len());
    let mut group = c.benchmark_group(group_name);
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    const N: usize = 10_0000;
    const BS: usize = 64 * KB;
    group.throughput(Throughput::Elements((N * vols.len()) as _));

    fn f(vol: Arc<Volume>) {
        file_records_iter::<BS>(&vol, N);
    }

    group.bench_function("single", |b| {
        b.iter(|| {
            for vol in &vols {
                f(vol.clone())
            }
        })
    });

    group.bench_function("multi", |b| {
        b.iter(|| {
            let mut handles = Vec::with_capacity(vols.len());
            for vol in &vols {
                let vol = Arc::clone(vol);
                let handle = spawn(|| f(vol));
                handles.push(handle);
            }

            for handle in handles {
                handle.join().unwrap();
            }
        })
    });

    group.finish();
}

criterion_group!(benches, file_records_buf, file_records_threading);
criterion_main!(benches);
