use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Instant,
};

use card_platypus::{
    alloc::tracking::{Stats, TrackingAllocator},
    collections::arraytree::ArrayTree,
    game::Action,
};

use dashmap::DashMap;
use itertools::Itertools;
use rand::{rngs::StdRng, Rng, SeedableRng};
use rayon::prelude::*;

pub fn run_and_track<T>(name: &str, size: usize, f: impl FnOnce(usize) -> T) {
    card_platypus::alloc::tracking::reset();

    let start = Instant::now();
    let t = f(size);
    let duration = start.elapsed();

    let Stats {
        alloc,
        dealloc,
        diff,
    } = card_platypus::alloc::tracking::stats();
    println!("{name},{size},{alloc},{dealloc},{diff}, {:?}", duration);

    drop(t);
}

pub fn main() {
    #[global_allocator]
    static ALLOC: TrackingAllocator = TrackingAllocator;

    println!("starting run...");
    let size = 3_000_000;
    run_and_track("mutex hashmap", size, mutex_hashmap_bench);
    run_and_track("dashmap", size, dashmap_bench);
    // Doing an extra allocation to convert to action list
    run_and_track("array tree", size, array_tree_bench);

    // 155_268_000
    // 2_245_231_328

    // 1_759_354_232
    // 92_146_699
    //130_617_360
    // 111_808_384
    // 83_379_462

    // message passing
    // track peak memory usage
    // track time to complete

    // work:
    // do a read, sleep for a bit (do some work)
    // write something
}

fn get_generator(len: usize) -> DataGenerator {
    DataGenerator::new(len, 42)
}

fn mutex_hashmap_bench(size: usize) -> Arc<Mutex<HashMap<Vec<u8>, Vec<f64>>>> {
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

fn dashmap_bench(size: usize) -> Arc<DashMap<Vec<u8>, Vec<f64>>> {
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

fn array_tree_bench(size: usize) -> Arc<ArrayTree<Vec<f64>>> {
    let x = Arc::new(ArrayTree::default());
    let generator = get_generator(size);
    generator.par_bridge().for_each(|(k, v)| {
        let s = x.clone();
        let key = k.iter().map(|x| Action(*x)).collect_vec();
        {
            s.get(&key);
        }
        do_work();
        s.insert(&key, v);
        drop(key);
    });
    x
}

fn do_work() {
    // std::thread::sleep(Duration::from_millis(1))
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
    type Item = (Vec<u8>, Vec<f64>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.count >= self.len {
            return None;
        }

        // Minimum key length is 6 to reflect the 6 dealt cards in euchre
        let key_length = self.rng.gen_range(6..20);
        let key: Vec<u8> = (0..key_length).map(|_| self.rng.gen_range(0..2)).collect();

        let data: Vec<f64> = (0..5).map(|_| self.rng.gen()).collect();
        self.count += 1;
        Some((key, data))
    }
}
