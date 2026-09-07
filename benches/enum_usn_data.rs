use std::{hint::black_box, thread, time::Duration};

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use fastfind::discover_drives;
use fastfind_ntfs::{enumerate::EnumUsnData, volume::Volume};

/// 拉取刚好 `limit` 条 USN 记录。
fn drain_records(iter: &mut EnumUsnData<'_>, limit: usize) -> (usize, usize, usize) {
    let mut records = 0;
    let mut buffers = 0;
    // 纯 USN 记录的字节数，不是缓冲区处理字节数，一般来说对每个缓冲区只少计入了前导 u64
    let mut bytes = 0;

    while records < limit {
        let Some(mut buffer) = iter.next_records().expect("`next_records` should work") else {
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

fn bench_threading(c: &mut Criterion) {
    const N: usize = 100_000;

    let volumes: Vec<Volume> = discover_drives()
        .into_iter()
        .filter_map(|drive| match Volume::open(drive.as_volume_path()) {
            Ok(volume) => Some(volume),
            Err(error) => {
                eprintln!("跳过驱动器 {}: {error}", drive.as_volume_path());
                None
            }
        })
        .collect();
    assert!(!volumes.is_empty(), "未找到可用 NTFS 卷");

    let group_name = format!("enum_usn_data/{}volumes", volumes.len());
    let mut group = c.benchmark_group(group_name);
    let elements = (volumes.len() * N) as _;
    group.throughput(Throughput::Elements(elements));
    group.warm_up_time(Duration::from_secs(2));
    group.sampling_mode(SamplingMode::Flat);
    group.sample_size(10);

    let scan_volume = |volume: &Volume| drain_records(&mut volume.enum_usn_data(), N);

    group.bench_function("serial", |b| {
        b.iter(|| {
            for volume in &volumes {
                scan_volume(volume);
            }
        });
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            thread::scope(|s| {
                for volume in &volumes {
                    s.spawn(|| scan_volume(volume));
                }
            })
        });
    });
}

criterion_group!(benches, bench_threading);
criterion_main!(benches);
