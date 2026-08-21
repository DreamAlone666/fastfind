use std::{hint::black_box, time::Duration};

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use fastfind_ntfs::{enumerate::EnumUsnData, volume::Volume};

const KB: usize = 1024;

/// 拉取刚好 `limit` 条 USN 记录。
fn drain_records(iter: &mut EnumUsnData<'_>, limit: usize) -> (usize, usize, usize) {
    let mut records = 0;
    let mut buffers = 0;
    // 纯 USN 记录的字节数，不是缓冲区处理字节数，一般来说对每个缓冲区只少计入了前导 u64
    let mut bytes = 0;

    while records < limit {
        let Some(mut buffer) = iter.next_buffer().expect("`next_buffer` should work") else {
            break;
        };
        buffers += 1;

        while let Some(record) = buffer.next_record() {
            let len = record.record_length() as usize;
            records += 1;
            bytes += len;

            if records >= limit {
                break;
            }
        }
    }

    assert_eq!(records, limit);

    black_box((records, buffers, bytes))
}

fn open_volume() -> Volume {
    const PATH: &str = r"\\.\C:";
    Volume::open(PATH).expect("Volume should be opened with correct path or admin privilege")
}

fn bench_capacity(c: &mut Criterion) {
    let volume = open_volume();

    const N: usize = 100_000;
    const CAPACITIES: [usize; 4] = [16 * KB, 32 * KB, 64 * KB, 128 * KB];

    let mut group = c.benchmark_group("enum_usn_data");

    group.warm_up_time(Duration::from_secs(2));
    // group.measurement_time(Duration::from_secs(5));

    // 只有当单次扫描很慢时再启用 Flat。
    // group.sampling_mode(SamplingMode::Flat);
    group.sample_size(20);

    for &capacity in &CAPACITIES {
        group.throughput(Throughput::Elements(N as u64));

        group.bench_with_input(
            BenchmarkId::new("capacity", capacity / KB),
            &capacity,
            |b, &capacity| {
                b.iter_batched_ref(
                    || EnumUsnData::with_capacity(&volume, capacity),
                    |iter| black_box(drain_records(iter, N)),
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_capacity);
criterion_main!(benches);
