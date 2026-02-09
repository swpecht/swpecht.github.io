use std::{
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{bail, Context};

use games::istate::IStateKey;
use log::{debug, warn};
use memmap2::MmapMut;

use crate::algorithms::cfres::InfoState;

use self::indexer::Indexer;

pub mod indexer;

const BUCKET_SIZE: usize = std::mem::size_of::<InfoState>();
const REMAP_INCREMENT: usize = 10_000_000;

const INDEXER_NAME: &str = "indexer";

// A performant, optionally diskback node storage system
pub struct NodeStore {
    indexer: Indexer,
    mmap: MmapMut,
    path: Option<PathBuf>,
    populated_count: AtomicUsize,
}

impl NodeStore {
    /// len is the number of infostates to provision for
    pub fn new_euchre(path: Option<&Path>, max_cards_played: usize) -> anyhow::Result<Self> {
        let mmap = get_mmap(path, 20_000_000).context("failed to create mmap")?;

        let path = path.map(|x| x.to_path_buf());
        let indexer = load_indexer(path.as_deref()).unwrap_or_else(|x| {
            warn!("failed to load indexer {}", x);
            Indexer::euchre(max_cards_played)
        });

        // Count existing populated entries when loading from disk
        let populated_count = count_populated(&mmap, &indexer);

        Ok(Self {
            indexer,
            mmap,
            path,
            populated_count: AtomicUsize::new(populated_count),
        })
    }

    pub fn new_kp(path: Option<&Path>) -> anyhow::Result<Self> {
        if path.is_some() {
            bail!("serialization not supported for this game type")
        }

        let mmap = get_mmap(path, 1_000)?;

        let path = path.map(|x| x.to_path_buf());
        Ok(Self {
            indexer: Indexer::kuhn_poker(),
            mmap,
            path,
            populated_count: AtomicUsize::new(0),
        })
    }

    pub fn new_bluff_11(path: Option<&Path>) -> anyhow::Result<Self> {
        if path.is_some() {
            bail!("serialization not supported for this game type")
        }
        let mmap = get_mmap(path, 10_000)?;

        let path = path.map(|x| x.to_path_buf());
        Ok(Self {
            indexer: Indexer::bluff_11(),
            mmap,
            path,
            populated_count: AtomicUsize::new(0),
        })
    }

    pub fn get(&self, key: &IStateKey) -> Option<InfoState> {
        let index: usize = self
            .indexer
            .index(key)
            .unwrap_or_else(|| panic!("failed to index {:?}", key));
        self.get_index(index)
    }

    fn get_index(&self, index: usize) -> Option<InfoState> {
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

        while start + BUCKET_SIZE >= self.mmap.len() {
            let cur_len = self.mmap.len() / BUCKET_SIZE;
            self.mmap.flush().expect("failed to flush mmap");
            self.mmap = get_mmap(self.path.as_deref(), cur_len + REMAP_INCREMENT)
                .expect("failed to resize mmap");
            debug!("resized mmap");
        }

        // Track new entries for O(1) len()
        let was_empty = self.mmap[start..start + BUCKET_SIZE].iter().all(|&x| x == 0);

        let value = [*value];
        let data = bytemuck::cast_slice::<InfoState, u8>(&value);
        assert!(data.len() <= BUCKET_SIZE); // if this is false, we're overflowing into another bucket
        self.mmap[start..start + data.len()].copy_from_slice(data);

        if was_empty {
            self.populated_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn commit(&mut self) -> anyhow::Result<()> {
        self.mmap.flush().context("failed to flush mmap")?;

        let Some(dir) = self.path.clone() else {
            return anyhow::Ok(());
        };

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(dir.join(INDEXER_NAME))?;

        let buf = serde_json::to_string(&self.indexer)?;
        file.write_all(buf.as_bytes())?;

        anyhow::Ok(())
    }

    /// Returns the number of populated items in the database. Not the total number of items
    pub fn len(&self) -> usize {
        self.populated_count.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the full size of the index, regardless of how many entries are populated
    pub fn indexer_len(&self) -> usize {
        self.indexer.len()
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

        // Don't change file size unless it is less than the target size
        let target_size = (len * BUCKET_SIZE) as u64;
        if file.metadata().context("failed to read file metadata")?.len() < target_size {
            file.set_len(target_size).context("failed to set length")?;
        }

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

/// Count the number of populated entries in an mmap
fn count_populated(mmap: &MmapMut, indexer: &Indexer) -> usize {
    let mut count = 0;
    for i in 0..indexer.len() {
        let start = i * BUCKET_SIZE;
        if start + BUCKET_SIZE <= mmap.len() {
            let data = &mmap[start..start + BUCKET_SIZE];
            if !data.iter().all(|&x| x == 0) {
                count += 1;
            }
        }
    }
    count
}

fn load_indexer(path: Option<&Path>) -> anyhow::Result<Indexer> {
    let Some(dir) = path else {
        bail!("no path");
    };

    let mut file = OpenOptions::new().read(true).open(dir.join(INDEXER_NAME))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;

    let indexer: Indexer = serde_json::from_str(&buf)?;
    anyhow::Ok(indexer)
}
