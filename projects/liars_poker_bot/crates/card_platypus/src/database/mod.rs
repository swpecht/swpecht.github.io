use std::{collections::HashSet, fs::OpenOptions, path::Path};

use anyhow::Context;
use boomphf::Mphf;
use itertools::Itertools;
use memmap2::MmapMut;

use crate::{
    actions,
    algorithms::cfres::InfoState,
    database::euchre_states::{generate_euchre_states, IStateBuilder},
    game::{bluff::Bluff, kuhn_poker::KuhnPoker, GameState},
    istate::IStateKey,
};

mod euchre_states;

const BUCKET_SIZE: usize = 200; // approximation of size of serialized infostate

// A performant, optionally diskback node storage system
pub struct NodeStore {
    phf: Mphf<IStateKey>,
    mmap: MmapMut,
}

impl NodeStore {
    /// len is the number of infostates to provision for
    pub fn new_euchre(file: Option<&Path>) -> anyhow::Result<Self> {
        let (phf, n) = generate_euchre_phf()?;
        let mmap = get_mmap(file, n)?;
        Ok(Self { phf, mmap })
    }

    pub fn new_kp(file: Option<&Path>) -> anyhow::Result<Self> {
        let (phf, n) = generate_phf(KuhnPoker::new_state)?;
        let mmap = get_mmap(file, n)?;
        Ok(Self { phf, mmap })
    }

    pub fn new_bluff_11(file: Option<&Path>) -> anyhow::Result<Self> {
        let (phf, n) = generate_phf(|| Bluff::new_state(1, 1))?;
        let mmap = get_mmap(file, n)?;
        Ok(Self { phf, mmap })
    }

    pub fn get(&self, key: &IStateKey) -> Option<InfoState> {
        let index: usize = self.phf.hash(key) as usize;
        let start = index * BUCKET_SIZE;
        let data = &self.mmap[start..start + BUCKET_SIZE];
        let info = match rmp_serde::from_slice(data) {
            Ok(x) => x,
            Err(_) => return None,
        };
        Some(info)
    }

    pub fn put(&mut self, key: &IStateKey, value: &InfoState) {
        let index: usize = self.phf.hash(key) as usize;
        let start = index * BUCKET_SIZE;

        let data = rmp_serde::to_vec(value).unwrap();
        assert!(data.len() <= BUCKET_SIZE); // if this is false, we're overflowing into another bucket
        self.mmap[start..start + data.len()].copy_from_slice(&data);
    }

    pub fn commit(&mut self) -> anyhow::Result<()> {
        self.mmap.flush().context("failed to flush mmap")
    }
}

fn get_mmap(file: Option<&Path>, len: usize) -> anyhow::Result<MmapMut> {
    if let Some(path) = file {
        let dir = path.parent().context("couldn't get file parent")?;
        std::fs::create_dir_all(dir)?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        file.set_len((len * BUCKET_SIZE) as u64).unwrap();
        Ok(unsafe { MmapMut::map_mut(&file).unwrap() })
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
