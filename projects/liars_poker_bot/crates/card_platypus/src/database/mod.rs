use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    path::{Path, PathBuf},
};

use anyhow::Context;
use boomphf::Mphf;
use itertools::Itertools;
use log::warn;
use memmap2::MmapMut;
use serde::{Deserialize, Serialize};

use crate::{
    actions,
    algorithms::cfres::InfoState,
    database::euchre_states::{generate_euchre_states, IStateBuilder},
    game::GameState,
    istate::IStateKey,
};

pub mod euchre_states;

const BUCKET_SIZE: usize = 200; // approximation of size of serialized infostate

#[derive(Default, Serialize, Deserialize)]
struct HashStore {
    index: HashMap<IStateKey, usize>,
    next: usize,
}

impl HashStore {
    pub fn hash(&mut self, key: &IStateKey) -> usize {
        match self.index.entry(*key) {
            std::collections::hash_map::Entry::Occupied(x) => return *x.get(),
            std::collections::hash_map::Entry::Vacant(x) => {
                let hash = self.next;
                self.next += 1;
                x.insert(hash);
                hash
            }
        }
    }

    pub fn get_hash(&self, key: &IStateKey) -> Option<usize> {
        self.index.get(key).copied()
    }

    pub fn len(&self) -> usize {
        self.next - 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// A performant, optionally diskback node storage system
pub struct NodeStore {
    // phf: Mphf<IStateKey>,
    phf: HashStore,
    mmap: MmapMut,
    path: Option<PathBuf>,
}

impl NodeStore {
    /// len is the number of infostates to provision for
    pub fn new_euchre(path: Option<&Path>) -> anyhow::Result<Self> {
        // let (phf, n) = generate_euchre_phf().context("failed to generate phf")?;

        let mmap = get_mmap(path, 10_000_000).context("failed to create mmap")?;
        let path = path.map(|x| x.to_path_buf());

        let phf = if let Some(path) = &path {
            let content = std::fs::read(path.join("index"))?;
            let phf: HashStore = rmp_serde::decode::from_slice(&content).unwrap_or_default();
            if phf.is_empty() {
                warn!("no index found or failed to load")
            }
            phf
        } else {
            HashStore::default()
        };

        Ok(Self { phf, mmap, path })
    }

    pub fn new_kp(path: Option<&Path>) -> anyhow::Result<Self> {
        // let (phf, n) = generate_phf(KuhnPoker::new_state)?;
        if path.is_some() {
            panic!("serialization not supported for this game type")
        }

        let mmap = get_mmap(path, 1_000)?;
        let phf = HashStore::default();
        let path = path.map(|x| x.to_path_buf());
        Ok(Self { phf, mmap, path })
    }

    pub fn new_bluff_11(path: Option<&Path>) -> anyhow::Result<Self> {
        // let (phf, n) = generate_phf(|| Bluff::new_state(1, 1))?;

        if path.is_some() {
            panic!("serialization not supported for this game type")
        }
        let mmap = get_mmap(path, 10_000)?;
        let phf = HashStore::default();
        let path = path.map(|x| x.to_path_buf());
        Ok(Self { phf, mmap, path })
    }

    pub fn get(&self, key: &IStateKey) -> Option<InfoState> {
        // let index: usize = self.phf.hash(key) as usize;
        let index: usize = self.phf.get_hash(key)?;
        let start = index * BUCKET_SIZE;

        if start + BUCKET_SIZE > self.mmap.len() {
            return None;
        }

        let data = &self.mmap[start..start + BUCKET_SIZE];
        let info = match rmp_serde::from_slice(data) {
            Ok(x) => x,
            Err(_) => return None,
        };
        Some(info)
    }

    pub fn put(&mut self, key: &IStateKey, value: &InfoState) {
        let index: usize = self.phf.hash(key);
        let start = index * BUCKET_SIZE;

        if start + BUCKET_SIZE > self.mmap.len() {
            todo!("Re-sizing not yet implemented")
        }

        let data = rmp_serde::to_vec(value).unwrap();
        assert!(data.len() <= BUCKET_SIZE); // if this is false, we're overflowing into another bucket
        self.mmap[start..start + data.len()].copy_from_slice(&data);
    }

    pub fn commit(&mut self) -> anyhow::Result<()> {
        self.mmap.flush().context("failed to flush mmap")?;

        if let Some(dir) = &self.path {
            let encoded = rmp_serde::to_vec(&self.phf)?;
            std::fs::write(dir.join("index"), encoded)?;
        }

        anyhow::Ok(())
    }

    pub fn len(&self) -> usize {
        self.phf.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn get_mmap(dir: Option<&Path>, len: usize) -> anyhow::Result<MmapMut> {
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).context("failed to create directory")?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(dir.join("mmap"))
            .context("failed to create mmap file")?;

        file.set_len((len * BUCKET_SIZE) as u64)
            .context("failed to set length")?;
        Ok(unsafe { MmapMut::map_mut(&file)? })
    } else {
        Ok(memmap2::MmapOptions::new()
            .len(len * BUCKET_SIZE)
            .map_anon()?)
    }
}

pub fn generate_euchre_phf() -> anyhow::Result<(Mphf<IStateKey>, usize)> {
    let mut builder = IStateBuilder::default();
    let mut istates = HashSet::new();
    generate_euchre_states(
        &mut builder,
        &mut istates,
        euchre_states::Termination::Play { cards: 1 },
    );

    let n = istates.len();
    let phf = Mphf::new_parallel(1.7, &istates.iter().copied().collect_vec(), None);
    validate_phf(&istates, &phf);

    Ok((phf, n))
}

fn generate_phf<T: GameState>(new_state: fn() -> T) -> anyhow::Result<(Mphf<IStateKey>, usize)> {
    let mut istates = HashSet::new();

    let mut gs = (new_state)();
    populate_all_states(&mut istates, &mut gs);

    let n = istates.len();
    let phf = Mphf::new_parallel(1.7, &istates.iter().copied().collect_vec(), None);
    validate_phf(&istates, &phf);

    Ok((phf, n))
}

fn populate_all_states<T: GameState>(istates: &mut HashSet<IStateKey>, gs: &mut T) {
    if gs.is_terminal() {
        return;
    }

    if !gs.is_chance_node() {
        let key = gs.istate_key(gs.cur_player());
        istates.insert(key);
    }

    let actions = actions!(gs);
    for a in actions {
        gs.apply_action(a);
        populate_all_states(istates, gs);
        gs.undo()
    }
}

fn validate_phf(istates: &HashSet<IStateKey>, phf: &Mphf<IStateKey>) {
    let n = istates.len();
    // Get hash value of all objects
    let mut hashes = Vec::new();
    for v in istates {
        hashes.push(phf.hash(v));
    }
    hashes.sort();

    // Expected hash output is set of all integers from 0..n
    let expected_hashes: Vec<u64> = (0..n as u64).collect();
    assert!(hashes == expected_hashes);
}
