use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::OpenOptions,
    sync::{Arc, Mutex},
    time::Instant,
};

use boomphf::Mphf;
use card_platypus::{
    algorithms::cfres::InfoState,
    alloc::tracking::{Stats, TrackingAllocator},
    collections::{actionlist::ActionList, actiontrie::ActionTrie},
    game::Action,
    istate::{IStateKey, NormalizedAction},
};

use dashmap::DashMap;

use itertools::Itertools;
use memmap2::MmapMut;
use rand::{rngs::StdRng, Rng, SeedableRng};
use rayon::prelude::*;

pub fn main() {
    #[global_allocator]
    static ALLOC: TrackingAllocator = TrackingAllocator;

    let size = 20_000_000;
    std::fs::create_dir_all("/tmp/card_platypus/").unwrap();

    println!("starting generation of phf");
    generate_phf("/tmp/card_platypus/phf", size).unwrap();

    println!("starting run...");

    run_and_track("mutex hashmap", size, mutex_hashmap_bench);
    run_and_track("dashmap", size, dashmap_bench);

    run_and_track("mem map w/ phf, single thread", size, mem_map);

    run_and_track("storage: actiontrie", 5_000_000, actiontrie_storage);
    run_and_track("storage: btree", 5_000_000, btree_storage);
    run_and_track("storage: btree vec", 5_000_000, btree_storage_vec);
}

pub fn run_and_track<T>(name: &str, size: usize, f: impl FnOnce(usize) -> T) {
    card_platypus::alloc::tracking::reset();

    let start = Instant::now();
    let t = f(size);
    let duration = start.elapsed();

    let Stats {
        alloc,
        dealloc,
        diff,
        peak,
    } = card_platypus::alloc::tracking::stats();
    println!(
        "{name},{size},{alloc},{dealloc},{diff},{peak}, {:?}",
        duration
    );

    drop(t);
}

fn get_generator(len: usize) -> DataGenerator {
    DataGenerator::new(len, 42)
}

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

fn dashmap_bench(size: usize) -> Arc<DashMap<IStateKey, InfoState>> {
    let x = Arc::new(DashMap::new());
    let generator = get_generator(size);
    generator.par_bridge().for_each(|(k, v)| {
        let s = x.clone();
        {
            s.get(&k);
        }
        do_work();
        s.insert(k, v);
    });
    x
}

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
        let _s: InfoState = rmp_serde::from_slice(data).unwrap_or(InfoState::new(vec![]));

        do_work();
        let data = rmp_serde::to_vec(&v).unwrap();
        assert!(data.len() <= BUCKET_SIZE); // if this is false, we're overflowing into another bucket
        mmap[start..start + data.len()].copy_from_slice(&data);
    });

    mmap.flush().unwrap();
}

fn btree_storage(size: usize) -> BTreeMap<IStateKey, usize> {
    let mut map = BTreeMap::new();

    let generator = get_generator(size);
    for (i, (k, _)) in generator.enumerate() {
        map.insert(k, i);
    }
    map
}

fn btree_storage_vec(size: usize) -> BTreeMap<Vec<Action>, usize> {
    let mut map = BTreeMap::new();

    let generator = get_generator(size);
    for (i, (k, _)) in generator.enumerate() {
        map.insert(k.to_vec(), i);
    }
    map
}

fn actiontrie_storage(size: usize) -> ActionTrie<usize> {
    let mut map = ActionTrie::default();

    let generator = get_generator(size);
    for (i, (k, _)) in generator.enumerate() {
        map.insert(&k, i);
    }
    map
}

fn do_work() {
    // std::thread::sleep(Duration::from_millis(1))
}

fn generate_phf(path: &str, size: usize) -> anyhow::Result<()> {
    let mut keys = HashSet::new();
    for (k, _) in get_generator(size) {
        keys.insert(k);
    }

    let n = keys.len();
    let phf = Mphf::new_parallel(1.7, &keys.iter().copied().collect_vec(), None);

    // Get hash value of all objects
    let mut hashes = Vec::new();
    for v in keys {
        hashes.push(phf.hash(&v));
    }
    hashes.sort();

    // Expected hash output is set of all integers from 0..n
    let expected_hashes: Vec<u64> = (0..n as u64).collect();
    assert!(hashes == expected_hashes);

    let serialized = rmp_serde::to_vec(&phf)?;
    std::fs::write(path, serialized)?;

    Ok(())
}

/// Generates random data for storage benchmarking
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
            actions: ActionList::new(
                &(0..5)
                    .map(|_| NormalizedAction::new(Action(self.rng.gen_range(0..32))))
                    .collect_vec(),
            ),
            regrets: (0..5).map(|_| self.rng.gen()).collect(),
            avg_strategy: (0..5).map(|_| self.rng.gen()).collect(),
            last_iteration: self.rng.gen(),
        };

        self.count += 1;
        Some((key, data))
    }
}
