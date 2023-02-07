use std::collections::HashMap;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use liars_poker_bot::database::{get_connection, read_data, write_data, Storage};
use rand::{distributions::Alphanumeric, Rng};
use sqlite::Connection;

fn write_page(data: HashMap<String, Vec<char>>) {
    let (mut c, t) = get_connection(Storage::Tempfile);
    write_data(&mut c, data);
    drop(t);
}

fn generate_data(n: usize) -> HashMap<String, Vec<char>> {
    let mut data: HashMap<String, Vec<char>> = HashMap::new();

    for _ in 0..n {
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

    return data;
}

fn read_database(c: &Connection) {
    let mut output: HashMap<String, Vec<char>> = HashMap::new();
    read_data(c, &"".to_string(), 99999, &mut output);

    assert_eq!(output.len(), 1000000);
}

fn criterion_benchmark(c: &mut Criterion) {
    let data = generate_data(100000);

    let mut group = c.benchmark_group("database-benchmarks");
    group.sample_size(10);
    group.bench_function("write page", |b| {
        b.iter(|| write_page(black_box(data.clone())))
    });

    let data = generate_data(1000000);
    let (mut c, t) = get_connection(Storage::Tempfile);
    write_data(&mut c, data);

    group.bench_function("read data", |b| b.iter(|| read_database(&c)));

    drop(t);
    group.finish()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
