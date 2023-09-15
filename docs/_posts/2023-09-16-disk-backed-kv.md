---
layout: post
title:  "Finding a disk-backed key-value store for a euchre bot"
categories: project-log
---

# Context

I began training my [euchre bot]({% post_url 2023-08-29-speeding-up-cfr-training %}) on more of the game -- expanding it from just the bidding phase where trump is called into the card play phase.

But I ran into a problem: the data needed to actually train the bot could no longer fit into memory. Storing just the bidding phase took 0.3GB of memory. Storing just up to the first 2 cards played required 6GB -- 20x the cost for 2 additional cards. We would likely need 100s of GBs to store the full 20 cards of play.

This post outlines my attempt to find the most performant disk-backed key-value store for training a euchre bot.

Much of the benchmark code for this is based on this excellent post: [Measuring the overhead of HashMaps in Rust](https://ntietz.com/blog/rust-hashmap-overhead/).

# The benchmark

For the workload, we need to:
* look up a key: a vector of `u8`s representing the actions taken so far
* `do_work`: here a no-op function
* Write a new value to the same key

See the appendix for the code to generate the benchmark data.

As an example of the workload, here is the benchmark code for a `HashMap` in a `Mutex`:

```rust
fn mutex_hashmap_bench(size: usize) -> Arc<Mutex<HashMap<IStateKey, InfoState>>> {
    let x = Arc::new(Mutex::new(HashMap::new()));
    let generator = get_generator(size);
    generator.par_bridge().for_each(|(k, v)| {
        let s = x.clone();
        {
            s.lock().unwrap().get(&k);
        }
        do_work();
        s.lock().unwrap().insert(k, v);
    });
    x
}
```

I ran all benchmarks on a Lenovo laptop with 16GB of RAM and a 10th gen i7. The results for 20m infostates for the in-memory store is:

| Storage          | Time (s) | Peak allocations (GB) |
| ---------------- | -------- | --------------------- |
| `Mutex(HashMap)` | ████ 38  | █████████ 8.9         |


The difference in allocations from the theoretical size is due to rusts doubling behavior when extending a hashmap ([Measuring the overhead of HashMaps in Rust](https://ntietz.com/blog/rust-hashmap-overhead/)).

# Don't you just need a database?

My first approach was to use a database. I evaluated both LMDB (with [heed](https://github.com/meilisearch/heed)) and RocksDB (with [rust-rocksdb](https://github.com/rust-rocksdb/rust-rocksdb)). I gave each database a cache of 1m keys so they could batch writes.

The LMDB code as an example:
```rust
fn heed_bench(size: usize) {
    let path = Path::new("/tmp/card_platypus").join("heed_bench.mdb");

    fs::create_dir_all(path.clone()).unwrap();

    let mut env_builder = EnvOpenOptions::new();
    unsafe {
        // Only sync meta data at the end of a transaction, this can hurt the durability
        // of the database, but cannot lead to corruption
        env_builder.flag(Flags::MdbNoMetaSync);
        // Disable OS read-ahead, can improve perf when db is larger than RAM
        env_builder.flag(Flags::MdbNoRdAhead);
        // Improves write performance, but can cause corruption if there is a bug in application
        // code that overwrite the memory address
        env_builder.flag(Flags::MdbWriteMap);
        // Avoid zeroing memory before use -- can cause issues with
        // sensitive data, but not a risk here.
        env_builder.flag(Flags::MdbNoMemInit);
    }
    const MAX_DB_SIZE_GB: usize = 10;
    env_builder.map_size(MAX_DB_SIZE_GB * 1024 * 1024 * 1024);

    let env = env_builder.open(path).unwrap();
    // need to open rather than create for WriteMap to work
    let x: Database<ByteSlice, SerdeBincode<InfoState>> = env.open_database(None).unwrap().unwrap();
    let cache = Mutex::new(HashMap::new());

    let generator = get_generator(size);

    generator.par_bridge().for_each(|(k, v)| {
        if cache.lock().unwrap().get(&k).is_none() {
            let rtxn = env.read_txn().unwrap();
            let k = k.as_slice().iter().map(|x| x.0).collect_vec();
            x.get(&rtxn, &k).unwrap();
        }

        do_work();
        cache.lock().unwrap().insert(k, v);

        let mut c = cache.lock().unwrap();
        if c.len() > 1_000_000 {
            let mut wtxn = env.write_txn().unwrap();
            for (k, v) in c.drain() {
                let k = k.as_slice().iter().map(|x| x.0).collect_vec();
                x.put(&mut wtxn, &k, &v).unwrap();
            }
            wtxn.commit().unwrap();
        }
    });
}
```

| Storage          | Time (s)                      | Peak allocations (GB) |
| ---------------- | ----------------------------- | --------------------- |
| `Mutex(HashMap)` | ████ 38                       | █████████ 8.9         |
| LMDB (heed)      | ███████████████████ 190       | █ 0.5                 |
| RocksDB          | █████████████████████████ 260 | █ 0.5                 |

The database backed approaches are 1/5 the speed of the memory only approach even with a cache that can hold 1/20th of the total keys (much better than possible in practice).

While tuning these benchmarks, I opted to turn on many of the "unsafe" features. For example, `WriteMap` for LMDB can improve write performance, but it can also allow application bugs to corrupt the database.

Given we aren't storing anything important in our database, can we go even faster if we get rid of even more of these pesky [ACID](https://en.wikipedia.org/wiki/ACID) features?

# Memory mapped files and perfect hash functions

When LMDB is in `WriteMap` mode, it uses a [memory mapped file](https://en.wikipedia.org/wiki/Memory-mapped_file) to directly update the entries. A memory mapped file let's us read and write the files contents as if it were fully loaded into memory.

Importantly it does not load the file into memory -- it works with files larger than RAM.

A memory mapped file solves part of the problem: it gives us a performant way to read and write a serialized version of our values to disk at specific locations. But it doesn't give us a way to tell which value is at a given location. Writing to the memory map requires a byte index, not a key like a hashmap.

There are many way to solve this. For example, LMDB uses a [B+ tree](https://en.wikipedia.org/wiki/B%2B_tree) to track where values are, and RocksDB uses an [LSM tree](https://en.wikipedia.org/wiki/Log-structured_merge-tree).

But we can do better by exploiting a difference in this problem from a traditional database: we know every possible key ahead of time.

We don't need to store arbitray data. We only need to store possible gamestates of euchre.

With this information, we can create a [perfect hash function](https://en.wikipedia.org/wiki/Perfect_hash_function) to map every key to exactly one index in our file. I used the [rust-boomphf](https://github.com/10XGenomics/rust-boomphf) to create the function.

Putting it all together:
```rust
fn mem_map(size: usize) {
    let serialized = std::fs::read("/tmp/card_platypus/phf").unwrap();
    let phf: Mphf<IStateKey> = rmp_serde::from_slice(&serialized).unwrap();

    let path = "/tmp/card_platypus/mem_map_bench";
    std::fs::create_dir_all("/tmp/card_platypus").unwrap();

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .unwrap();

    const BUCKET_SIZE: usize = 200; // approximation of size of serialized infostate
    file.set_len((size * BUCKET_SIZE) as u64).unwrap();
    let mut mmap = unsafe { MmapMut::map_mut(&file).unwrap() };

    let generator = get_generator(size);
    generator.for_each(|(k, v)| {
        let index: usize = phf.hash(&k) as usize;

        let start = index * BUCKET_SIZE;
        let data = &mmap[start..start + BUCKET_SIZE];
        let s: InfoState = rmp_serde::from_slice(data).unwrap_or(InfoState::new(vec![]));

        do_work();
        let data = rmp_serde::to_vec(&v).unwrap();
        assert!(data.len() <= BUCKET_SIZE); // if this is false, we're overflowing into another bucket
        mmap[start..start + data.len()].copy_from_slice(&data);
    });

    mmap.flush().unwrap();
}
```

And the benchmark results:

| Storage          | Time (s)                      | Peak allocations (GB) |
| ---------------- | ----------------------------- | --------------------- |
| `Mutex(HashMap)` | ████ 38                       | █████████ 8.9         |
| LMDB (heed)      | ███████████████████ 190       | █ 0.5                 |
| RocksDB          | █████████████████████████ 260 | █ 0.5                 |
| MemMap w/ phf    | █████ 54                      | \|<0.1                |

This approach is only 1.4x the time of the in memory approach as compared to the 5x+ time for the database approach.

But it didn't come for free -- its sacrifices the correctness guarantees of a traditional database. Hopefully I don't come to regret not having those later :)

## Appendix

# Infostate definition

The infostates are about 165 bytes of data total on average [0]. We need to store one of these for every state the CFR algorithm need to evaluate.

```rust
pub struct InfoState {
    actions: Vec<u8>, // 24 bytes for vector + 5 * 1 for each u8 action
    regrets: Vec<f64>, // 24 bytes for vector + 5 * 8 for each f64 regret)
    avg_strategy: Vec<f64>, // 24 bytes
    last_iteration: usize, // 8 bytes
}
```


[0] There are some things we could do to reduce this size, e.g. switching to `f32` instead of `f64` or using a bit mask to keep track of which actions are present rather than storing a vector of actions, but none of these changes would be enough to fit everything into RAM for a full game of euchre.

# Code for generating benchmark data

```rust
struct DataGenerator {
    rng: StdRng,
    len: usize,
    count: usize,
}

impl DataGenerator {
    fn new(len: usize, seed: u64) -> Self {
        Self {
            rng: SeedableRng::seed_from_u64(seed),
            len,
            count: 0,
        }
    }
}

impl Iterator for DataGenerator {
    type Item = (IStateKey, InfoState);

    fn next(&mut self) -> Option<Self::Item> {
        if self.count >= self.len {
            return None;
        }

        // Minimum key length is 6 to reflect the 6 dealt cards in euchre
        let key_length = self.rng.gen_range(6..20);
        let mut key = IStateKey::default();
        (0..key_length).for_each(|_| key.push(Action(self.rng.gen_range(0..32))));

        let data = InfoState {
            actions: (0..5)
                .map(|_| NormalizedAction::new(Action(self.rng.gen_range(0..32))))
                .collect(),
            regrets: (0..5).map(|_| self.rng.gen()).collect(),
            avg_strategy: (0..5).map(|_| self.rng.gen()).collect(),
            last_iteration: self.rng.gen(),
        };

        self.count += 1;
        Some((key, data))
    }
}
```