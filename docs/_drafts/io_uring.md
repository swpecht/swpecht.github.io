---
layout: post
title:  "Speeding up IO in Rust with io_uring"
categories: project-log
---

## Context

I'm working on creating a bot to play the card game euchre using counter-factual regret minimization. To do this, I need a way to store the vast number of game nodes for training. They can't all fit in memory. This post explores how I used io_uring to speed up IO compared to using a sqlite3.

As a caveat: (not generalizable comparison, worked only for my usecase)

## The benchmark

For the euchre bot, I need to read and write hashmaps of game nodes. I create toy data using the following function:

```Rust
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
```

I then use criterion to run the benchmarks:

## sqlite3 implementation

```Rust
pub fn write_data<T: Serialize>(c: &mut Connection, items: HashMap<String, T>) {
    const INSERT_QUERY: &str =
        "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";

    // Use a transaction for performance reasons
    c.execute("BEGIN TRANSACTION;").unwrap();

    for (k, v) in items.iter() {
        let s = serde_json::to_string(v).unwrap();
        let mut statement = c.prepare(INSERT_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", k.clone().into()), (":node", s.into())][..])
            .unwrap();
        let r = statement.next();
        if !r.is_ok() {
            panic!("{:?}", r);
        }
    }

    while c.execute("COMMIT;").is_err() {
        warn!("retrying write, database errord on commit");
    }
}

pub fn read_data<T>(c: &Connection, key: &String, max_len: usize, output: &mut HashMap<String, T>)
where
    T: DeserializeOwned,
{
    const LOAD_PAGE_QUERY: &str =
        "SELECT * FROM nodes WHERE istate LIKE :like AND LENGTH(istate) <= :maxlen;";

    // We are manually concatenating the '%' character in rust code to ensure that we are
    // performing the LIKE query against a string literal. This allows sqlite to use the
    // index for this query.
    let mut statement = c.prepare(LOAD_PAGE_QUERY).unwrap();
    let mut like_statement = key.clone();
    like_statement.push('%');
    statement
        .bind::<&[(_, Value)]>(
            &[
                (":like", like_statement.into()),
                (":maxlen", (max_len as i64).into()),
            ][..],
        )
        .unwrap();

    while let Ok(State::Row) = statement.next() {
        let node_ser = statement.read::<String, _>("node").unwrap();
        let istate = statement.read::<String, _>("istate").unwrap();
        let node = serde_json::from_str(&node_ser).unwrap();
        output.insert(istate, node);
    }
}
```

## io_uring implementation

## Results

All results are for writing and reading 1M entries.
<pre>
Approach               Write time (s)        Read time (s)
sqlite                |████████████▌ 12.4   |██▌ 2.4
BufWriter             |▌ 0.8                |██ 1.9
io_uring, 4kb buffer  |██ 2.8               |██ 2.0
io_uring, 64kb buffer |▌ 0.8                |██ 1.9
</pre>

In addition to improvement on micro-benchmarks, this change improved overall

Impact on overall performance:

sqlite:
    2023-02-09T16:38:44-07:00 - INFO - Starting self play for CFR
    2023-02-09T16:40:08-07:00 - DEBUG - finished training policy (0:01:24)

io_uring:
    2023-02-09T16:36:12-07:00 - INFO - Starting self play for CFR
    2023-02-09T16:37:03-07:00 - DEBUG - finished training policy (0:00:51)

## Isn't the io_uring missing a lot of sqlite functionality?

## Appendix: Benchmark details

### Benchmark code

```Rust
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
```

### Benchmark results

```Bash
     Running benches/database_benchmarks.rs (target/release/deps/database_benchmarks-9874057142d6bc60)
Benchmarking database-benchmarks/sql backend read: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 23.9s.
database-benchmarks/sql backend read
                        time:   [2.2431 s 2.4304 s 2.6393 s]
Benchmarking database-benchmarks/file backend read: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 22.0s.
database-benchmarks/file backend read
                        time:   [1.8314 s 1.9063 s 2.0202 s]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high severe
Benchmarking database-benchmarks/io_uring backend read, 4kb: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 23.1s.
database-benchmarks/io_uring backend read, 4kb
                        time:   [1.9300 s 1.9999 s 2.0944 s]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high severe
Benchmarking database-benchmarks/io_uring backend read, 64kb: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 22.1s.
Benchmarking database-benchmarks/io_uring backend read, 64kb: Collecting 10 samples in estimated 22.093 s (10 iterationsdatabase-benchmarks/io_uring backend read, 64kb
                        time:   [1.8222 s 1.9069 s 2.0186 s]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high severe
Benchmarking database-benchmarks/sql backend write: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 73.8s.
database-benchmarks/sql backend write
                        time:   [12.022 s 12.465 s 12.973 s]
Benchmarking database-benchmarks/file backend write: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 7.2s.
database-benchmarks/file backend write
                        time:   [725.99 ms 785.43 ms 855.06 ms]
Benchmarking database-benchmarks/io_uring backend write, 4kb: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 30.6s.
Benchmarking database-benchmarks/io_uring backend write, 4kb: Collecting 10 samples in estimated 30.583 s (10 iterationsdatabase-benchmarks/io_uring backend write, 4kb
                        time:   [2.7368 s 2.8441 s 2.9589 s]
Benchmarking database-benchmarks/io_uring backend write, 64kb: Warming up for 3.0000 s
Warning: Unable to complete 10 samples in 5.0s. You may wish to increase target time to 8.9s.
Benchmarking database-benchmarks/io_uring backend write, 64kb: Collecting 10 samples in estimated 8.9492 s (10 iterationdatabase-benchmarks/io_uring backend write, 64kb
                        time:   [820.30 ms 835.01 ms 856.19 ms]
Found 1 outliers among 10 measurements (10.00%)
  1 (10.00%) high severe

     Running benches/game_benchmarks.rs (target/release/deps/game_benchmarks-e8918e3edc11b017)
```
