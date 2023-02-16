use criterion::{
    black_box, criterion_group, criterion_main, measurement::Measurement, BenchmarkGroup, Criterion,
};
use liars_poker_bot::database::{
    disk_backend::DiskBackend, file_backend::FileBackend, io_uring_backend::UringBackend,
    page::Page, sqlite_backend::SqliteBackend, Storage,
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

fn benchmark_write<T: Measurement, B>(group: &mut BenchmarkGroup<T>, name: &str, mut backend: B)
where
    B: DiskBackend<Vec<char>>,
{
    let data = generate_page("", 1_000_000);
    group.bench_function(name, |b| {
        b.iter(|| backend.write_sync(black_box(data.clone())))
    });
}

fn benchmark_read<T: Measurement, B>(group: &mut BenchmarkGroup<T>, name: &str, mut backend: B)
where
    B: DiskBackend<Vec<char>>,
{
    let data = generate_page("", 1_000_000);
    backend.write_sync(data).unwrap();

    group.bench_function(name, |b| {
        b.iter(|| {
            let mut p = Page::new("", &[]);
            p = backend.read(p);
            assert_eq!(p.cache.len(), 1_000_000)
        })
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("database-benchmarks");
    group.sample_size(10);

    benchmark_read(
        &mut group,
        "sql backend read",
        SqliteBackend::new(Storage::Temp),
    );
    benchmark_read(
        &mut group,
        "file backend read",
        FileBackend::new(Storage::Temp),
    );
    benchmark_read(
        &mut group,
        "io_uring backend read, 4kb",
        UringBackend::new_with_buffer_size(Storage::Temp, 4096),
    );
    benchmark_read(
        &mut group,
        "io_uring backend read, 64kb",
        UringBackend::new_with_buffer_size(Storage::Temp, 65536),
    );

    benchmark_write(
        &mut group,
        "sql backend write",
        SqliteBackend::new(Storage::Temp),
    );
    benchmark_write(
        &mut group,
        "file backend write",
        FileBackend::new(Storage::Temp),
    );
    benchmark_write(
        &mut group,
        "io_uring backend write, 4kb",
        UringBackend::new_with_buffer_size(Storage::Temp, 4096),
    );
    benchmark_write(
        &mut group,
        "io_uring backend write, 64kb",
        UringBackend::new_with_buffer_size(Storage::Temp, 65536),
    );

    group.finish()
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
