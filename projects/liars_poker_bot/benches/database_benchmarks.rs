use std::collections::HashMap;

use criterion::{criterion_group, criterion_main, Criterion};
use liars_poker_bot::database::{get_connection, write_data, Storage};
use rand::{distributions::Alphanumeric, Rng};

fn write_page() {
    // create data to write

    let mut data: HashMap<String, Vec<char>> = HashMap::new();
    for _ in 0..100000 {
        let k: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(20)
            .map(char::from)
            .collect();
        let v: Vec<char> = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(20)
            .map(char::from)
            .collect();
        data.insert(k, v);
    }

    let (mut c, t) = get_connection(Storage::Tempfile);
    write_data(&mut c, data);
    drop(t);
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("database-benchmarks");
    group.sample_size(10);
    group.bench_function("write page", |b| b.iter(|| write_page()));
    group.finish()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
