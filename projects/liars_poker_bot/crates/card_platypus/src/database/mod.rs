use std::{
    collections::{btree_map::Entry, BTreeMap, HashSet},
    fs::OpenOptions,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context};
use boomphf::Mphf;
use games::{
    actions,
    gamestates::{
        bluff::{Bluff, BluffGameState},
        euchre::iterator::EuchreIsomorphicIStateIterator,
        kuhn_poker::{KPGameState, KuhnPoker},
    },
    istate::IStateKey,
    iterator::IStateIterator,
    Action, GameState,
};
use itertools::Itertools;
use log::{debug, warn};
use memmap2::MmapMut;
use serde::{Deserialize, Serialize};

use crate::algorithms::cfres::InfoState;

use self::indexer::Indexer;

pub mod indexer;

const BUCKET_SIZE: usize = std::mem::size_of::<InfoState>();
const REMAP_INCREMENT: usize = 10_000_000;

/// We use a vectorized version of the istates instead of the array to reduce memory usage
#[derive(Default, Serialize, Deserialize)]
struct HashStore {
    index: BTreeMap<Vec<Action>, usize>,
    next: usize,
}

impl HashStore {
    pub fn hash(&mut self, key: &IStateKey) -> usize {
        let key = key.to_vec();
        match self.index.entry(key) {
            Entry::Occupied(x) => return *x.get(),
            Entry::Vacant(x) => {
                let hash = self.next;
                self.next += 1;
                x.insert(hash);
                hash
            }
        }
    }

    pub fn get_hash(&self, key: &IStateKey) -> Option<usize> {
        let key = key.to_vec();
        self.index.get(&key).copied()
    }

    pub fn len(&self) -> usize {
        self.next
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// A performant, optionally diskback node storage system
pub struct NodeStore {
    indexer: Indexer,
    mmap: MmapMut,
    path: Option<PathBuf>,
}

impl NodeStore {
    /// len is the number of infostates to provision for
    pub fn new_euchre(path: Option<&Path>) -> anyhow::Result<Self> {
        let mmap = get_mmap(path, 20_000_000).context("failed to create mmap")?;

        let path = path.map(|x| x.to_path_buf());
        Ok(Self {
            indexer: Indexer::euchre(4),
            mmap,
            path,
        })
    }

    pub fn new_kp(path: Option<&Path>) -> anyhow::Result<Self> {
        // let (phf, n) = generate_phf(KuhnPoker::new_state)?;
        if path.is_some() {
            panic!("serialization not supported for this game type")
        }

        let mmap = get_mmap(path, 1_000)?;

        let path = path.map(|x| x.to_path_buf());
        Ok(Self {
            indexer: Indexer::kuhn_poker(),
            mmap,
            path,
        })
    }

    pub fn new_bluff_11(path: Option<&Path>) -> anyhow::Result<Self> {
        if path.is_some() {
            panic!("serialization not supported for this game type")
        }
        let mmap = get_mmap(path, 10_000)?;

        let path = path.map(|x| x.to_path_buf());
        Ok(Self {
            indexer: Indexer::bluff_11(),
            mmap,
            path,
        })
    }

    pub fn get(&self, key: &IStateKey) -> Option<InfoState> {
        let index: usize = self
            .indexer
            .index(key)
            .unwrap_or_else(|| panic!("failed to index {:?}", key));
        let start = index * BUCKET_SIZE;

        if start + BUCKET_SIZE > self.mmap.len() {
            return None;
        }

        let data = &self.mmap[start..start + BUCKET_SIZE];

        // Check if the data is uninitialized
        if data.iter().all(|&x| x == 0) {
            return None;
        }

        let info = bytemuck::cast_slice::<u8, InfoState>(data)[0];
        Some(info)
    }

    pub fn put(&mut self, key: &IStateKey, value: &InfoState) {
        let index: usize = self
            .indexer
            .index(key)
            .unwrap_or_else(|| panic!("failed to index {:?}", key));
        let start = index * BUCKET_SIZE;

        if start + BUCKET_SIZE > self.mmap.len() {
            let cur_len = self.mmap.len() / BUCKET_SIZE;
            self.mmap.flush().expect("failed to flush mmap");
            self.mmap = get_mmap(self.path.as_deref(), cur_len + REMAP_INCREMENT)
                .expect("failed to resize mmap");
            debug!("resized mmap");
        }

        // let data = rmp_serde::to_vec(value).unwrap();
        let value = [*value];
        let data = bytemuck::cast_slice::<InfoState, u8>(&value);
        assert!(data.len() <= BUCKET_SIZE); // if this is false, we're overflowing into another bucket
        self.mmap[start..start + data.len()].copy_from_slice(data);
    }

    pub fn commit(&mut self) -> anyhow::Result<()> {
        self.mmap.flush().context("failed to flush mmap")?;

        anyhow::Ok(())
    }

    /// TODO: fix this
    pub fn len(&self) -> usize {
        // self.phf.len()
        0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn get_mmap(dir: Option<&Path>, len: usize) -> anyhow::Result<MmapMut> {
    let mmap = if let Some(dir) = dir {
        std::fs::create_dir_all(dir).context("failed to create directory")?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(dir.join("mmap"))
            .context("failed to create mmap file")?;

        file.set_len((len * BUCKET_SIZE) as u64)
            .context("failed to set length")?;
        unsafe { MmapMut::map_mut(&file)? }
    } else {
        memmap2::MmapOptions::new()
            .len(len * BUCKET_SIZE)
            .map_anon()?
    };

    // Inform that re-ahead may not be useful
    mmap.advise(memmap2::Advice::Random)?;
    Ok(mmap)
}

fn load_phf(path: Option<&Path>) -> anyhow::Result<HashStore> {
    if let Some(path) = &path {
        let content = std::fs::read(path.join("index"))?;
        let phf: HashStore = rmp_serde::decode::from_slice(&content)?;
        Ok(phf)
    } else {
        bail!("path is none")
    }
}
