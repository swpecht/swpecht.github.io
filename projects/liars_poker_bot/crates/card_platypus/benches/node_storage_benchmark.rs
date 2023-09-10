use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

use card_platypus::{
    alloc::tracking::{Stats, TrackingAllocator},
    cfragent::cfres::InfoState,
    collections::diskstore::DiskStore,
    game::Action,
    istate::{IStateKey, NormalizedAction},
};

use dashmap::DashMap;

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
        peak,
    } = card_platypus::alloc::tracking::stats();
    println!(
        "{name},{size},{alloc},{dealloc},{diff},{peak}, {:?}",
        duration
    );

    drop(t);
}

pub fn main() {
    #[global_allocator]
    static ALLOC: TrackingAllocator = TrackingAllocator;

    println!("starting run...");
    let size = 10_000_000;
    run_and_track("mutex hashmap", size, mutex_hashmap_bench);
    run_and_track("dashmap", size, dashmap_bench);

    run_and_track("heed", size, heed_bench);
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

fn heed_bench(size: usize) -> DiskStore {
    let path = Path::new("/tmp/card_platypus").join("bytemuck.mdb");
    let x = DiskStore::new(Some(&path)).unwrap();
    println!("heed len: {}", x.len());

    let generator = get_generator(size);

    generator.for_each(|(k, v)| {
        {
            x.get(&k);
        }
        do_work();
        x.put(k, v);
    });

    x.commit();
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
