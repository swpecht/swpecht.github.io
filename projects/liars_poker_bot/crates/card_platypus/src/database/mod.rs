use std::{collections::HashSet, fs::OpenOptions, path::Path};

use anyhow::Context;
use boomphf::Mphf;
use itertools::Itertools;
use memmap2::MmapMut;

use crate::{
    database::euchre_states::{generate_euchre_states, IStateBuilder},
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
    pub fn new(phf: &Path, file: Option<&Path>, len: usize) -> anyhow::Result<Self> {
        let serialized = std::fs::read(phf)?;
        let phf: Mphf<IStateKey> = rmp_serde::from_slice(&serialized)?;

        let mmap = if let Some(path) = file {
            let dir = path.parent().context("couldn't get file parent")?;
            std::fs::create_dir_all(dir)?;

            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)?;

            file.set_len((len * BUCKET_SIZE) as u64).unwrap();
            unsafe { MmapMut::map_mut(&file).unwrap() }
        } else {
            memmap2::MmapOptions::new().map_anon()?
        };

        Ok(Self { phf, mmap })
    }
}

pub fn generate_euchre_phf(path: &Path) -> anyhow::Result<usize> {
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

    let serialized = rmp_serde::to_vec(&phf)?;
    std::fs::write(path, serialized)?;

    Ok(n)
}

fn validate_phf(istates: &HashSet<IStateKey>, phf: &Mphf<IStateKey>) {
    let n = istates.len();
    // Get hash value of all objects
    let mut hashes = Vec::new();
    for v in istates {
        hashes.push(phf.hash(&v));
    }
    hashes.sort();

    // Expected hash output is set of all integers from 0..n
    let expected_hashes: Vec<u64> = (0..n as u64).collect();
    assert!(hashes == expected_hashes);
}
