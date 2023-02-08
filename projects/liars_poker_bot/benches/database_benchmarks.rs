use std::collections::HashMap;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use liars_poker_bot::database::{io_uring_backend, sqlite_backend, Storage};
use rand::{distributions::Alphanumeric, Rng};
use sqlite::Connection;

fn sql_write_page(data: HashMap<String, Vec<char>>) {
    let (mut c, t, _) = sqlite_backend::get_connection(Storage::Tempfile);
    sqlite_backend::write_data(&mut c, data);
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

fn sql_read_database(c: &Connection) {
    let mut output: HashMap<String, Vec<char>> = HashMap::new();
    sqlite_backend::read_data(c, &"".to_string(), 99999, &mut output);

    assert_eq!(output.len(), 1000000);
}

fn io_uring_write_page(data: HashMap<String, Vec<char>>) {
    io_uring_backend::write_data(data).unwrap();
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("database-benchmarks");
    group.sample_size(10);
    let data = generate_data(1000000);
    group.bench_function("sql write page", |b| {
        b.iter(|| sql_write_page(black_box(data.clone())))
    });

    let (mut c, t, _) = sqlite_backend::get_connection(Storage::Tempfile);
    sqlite_backend::write_data(&mut c, data.clone());

    group.bench_function("sql read data", |b| b.iter(|| sql_read_database(&c)));

    drop(t);

    group.bench_function("io_uring write page", |b| {
        b.iter(|| io_uring_write_page(black_box(data.clone())))
    });

    group.finish()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
