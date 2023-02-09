use criterion::{black_box, criterion_group, criterion_main, Criterion};
use liars_poker_bot::database::{
    disk_backend::DiskBackend, io_uring_backend::UringBackend, page::Page,
    sqlite_backend::SqliteBackend, Storage,
};
use rand::{distributions::Alphanumeric, Rng};

fn generate_page(istate: &str, n: usize) -> Page<Vec<char>> {
    let mut p = Page::new(istate, &[]);

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
        p.cache.insert(k, v);
    }

    return p;
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("database-benchmarks");
    group.sample_size(10);
    let data = generate_page("", 1_000_000);
    let mut sql = SqliteBackend::new(Storage::Temp);

    group.bench_function("sql write data", |b| {
        b.iter(|| sql.write_sync(black_box(data.clone())))
    });

    let mut sql = SqliteBackend::new(Storage::Temp);
    sql.write_sync(data.clone()).unwrap();

    group.bench_function("sql read data", |b| {
        b.iter(|| {
            let mut p = Page::new("", &[]);
            p = sql.read(p);
            assert_eq!(p.cache.len(), 1_000_000)
        })
    });

    let data = generate_page("", 1_000_000);
    let mut io_uring = UringBackend::new_with_buffer_size(Storage::Temp, 65536);
    group.bench_function("io_uring write data, 64kb", |b| {
        b.iter(|| io_uring.write(black_box(data.clone())))
    });

    let mut io_uring = UringBackend::new_with_buffer_size(Storage::Temp, 4096);
    group.bench_function("io_uring write data, 4kb", |b| {
        b.iter(|| io_uring.write(black_box(data.clone())))
    });

    let mut io_uring = UringBackend::new_with_buffer_size(Storage::Temp, 65536);
    io_uring.write(data).unwrap();

    group.bench_function("io_uring read data, 64kb", |b| {
        b.iter(|| {
            let mut p: Page<Vec<char>> = Page::new("", &[]);
            p = io_uring.read(p);
            assert_eq!(p.cache.len(), 1_000_000)
        })
    });

    let data = generate_page("", 1_000_000);
    let mut io_uring = UringBackend::new_with_buffer_size(Storage::Temp, 4096);
    io_uring.write(data).unwrap();

    group.bench_function("io_uring read data, 4kb", |b| {
        b.iter(|| {
            let mut p: Page<Vec<char>> = Page::new("", &[]);
            p = io_uring.read(p);
            assert_eq!(p.cache.len(), 1_000_000)
        })
    });

    group.finish()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
