use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use memchr::memmem::Finder;
use std::{hint::black_box, time::Duration};

const QUERY: &[u8] = b"qzqzqzqz";
// 不包含 q/z，保证测试数据不会意外命中 QUERY。
const ALPHABET: &[u8] = b"AbCdEfGhIjKlMnOpRsTuVwXy0123456789_";

fn make_names(count: usize, len: usize) -> Vec<Box<str>> {
    (0..count)
        .map(|i| {
            // 用数字前缀保证每个名字不同；后缀只负责提供稳定的混合大小写 ASCII 数据。
            let mut name = format!("F{i}_");
            assert!(name.len() <= len);
            name.reserve(len - name.len());

            for j in name.len()..len {
                name.push(ALPHABET[j % ALPHABET.len()] as char);
            }

            name.into_boxed_str()
        })
        .collect()
}

fn scan_to_ascii_lowercase(names: &[Box<str>], finder: &Finder<'_>) -> usize {
    let names = black_box(names);
    let mut hits = 0;

    for name in names {
        let lower = name.to_ascii_lowercase();
        if finder.find(lower.as_bytes()).is_some() {
            hits += 1;
        }
    }

    black_box(hits)
}

fn scan_reuse_buffer(names: &[Box<str>], finder: &Finder<'_>) -> usize {
    let names = black_box(names);
    let mut lower = String::new();
    let mut hits = 0;

    for name in names {
        lower.clear();
        lower.push_str(name);
        lower.make_ascii_lowercase();

        if finder.find(lower.as_bytes()).is_some() {
            hits += 1;
        }
    }

    black_box(hits)
}

fn ascii_lowercase(c: &mut Criterion) {
    let mut group = c.benchmark_group("find/ascii_lowercase");
    // 1m 条记录的单次扫描较重，10 个样本足够观察数量级，也避免 Criterion 超时。
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));

    // 100k/1m 更接近全盘搜索；255 字节用于长文件名压力测试。
    for (count, len) in [(100_000, 64), (1_000_000, 64), (100_000, 255)] {
        let names: Vec<Box<str>> = make_names(count, len);
        let finder = Finder::new(QUERY).into_owned();
        group.throughput(Throughput::Bytes((count * len) as u64));

        group.bench_function(format!("to/{count}x{len}"), |b| {
            b.iter(|| scan_to_ascii_lowercase(&names, &finder))
        });
        group.bench_function(format!("reuse/{count}x{len}"), |b| {
            b.iter(|| scan_reuse_buffer(&names, &finder))
        });
    }

    group.finish();
}

criterion_group!(benches, ascii_lowercase);
criterion_main!(benches);
