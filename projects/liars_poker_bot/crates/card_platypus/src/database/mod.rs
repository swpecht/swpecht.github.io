use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{bail, Context};

use games::istate::IStateKey;
use log::{debug, warn};
use memmap2::MmapMut;

use crate::algorithms::cfres::{
    InfoState, BLUFF_MAX_ACTIONS, EUCHRE_MAX_ACTIONS, KP_MAX_ACTIONS, OH_MAX_ACTIONS,
};

use self::indexer::Indexer;

pub mod indexer;

const REMAP_INCREMENT: usize = 10_000_000;
const INDEXER_NAME: &str = "indexer";
const METADATA_NAME: &str = "meta";

/// Pluggable storage for CFR infostates. Two backings:
///   - `Mmap`: a perfect-hash + mmap-backed array. Fast O(1) get/put, but
///     requires pre-enumeration of every reachable infostate to build the
///     PHF. Used for games where that enumeration is tractable
///     (Kuhn Poker, Bluff(1,1), Euchre with isomorphic reduction).
///   - `Hash`: a plain `HashMap`. Lazily populated, no pre-enumeration
///     needed. Used for games whose istate space is too large to enumerate
///     (e.g. Oh Hell with the full 52-card deck). Optionally backed by a
///     MessagePack file on disk for checkpoint / resume.
pub enum NodeStore<const MAX_ACTIONS: usize> {
    Mmap(MmapBacking<MAX_ACTIONS>),
    Hash(HashBacking<MAX_ACTIONS>),
}

/// In-memory infostate store with optional disk persistence. `commit()`
/// serialises the `HashMap` to `path` via MessagePack; the constructor
/// hydrates from `path` if the file exists.
pub struct HashBacking<const MAX_ACTIONS: usize> {
    map: HashMap<IStateKey, InfoState<MAX_ACTIONS>>,
    path: Option<PathBuf>,
}

impl<const MAX_ACTIONS: usize> HashBacking<MAX_ACTIONS> {
    /// New empty store (with optional save path). If `path` exists, hydrate
    /// from it; otherwise start with an empty map.
    pub fn new(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let mut map = HashMap::new();
        if let Some(p) = path.as_ref() {
            if p.exists() {
                let bytes = std::fs::read(p)
                    .with_context(|| format!("reading {}", p.display()))?;
                map = rmp_serde::from_slice(&bytes)
                    .with_context(|| format!("decoding {}", p.display()))?;
                debug!("HashBacking: loaded {} infostates from {}", map.len(), p.display());
            }
        }
        Ok(Self { map, path })
    }

    pub fn get(&self, key: &IStateKey) -> Option<InfoState<MAX_ACTIONS>> {
        self.map.get(key).copied()
    }

    pub fn put(&mut self, key: &IStateKey, value: &InfoState<MAX_ACTIONS>) {
        self.map.insert(*key, *value);
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Serialise to disk if a path was provided. Writes via a `.tmp` file
    /// + rename so a half-written checkpoint can't replace a good one.
    pub fn commit(&self) -> anyhow::Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating dir {}", parent.display()))?;
            }
        }
        let bytes = rmp_serde::to_vec(&self.map)
            .context("encoding HashBacking map")?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &bytes)
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }
}

/// Mmap + PHF backing for `NodeStore`.
///
/// `MAX_ACTIONS` is the maximum number of actions stored per slot, which
/// controls the on-disk size of each bucket. Each game gets a right-sized
/// slot so Euchre (max 6 actions) doesn't pay Bluff's (max ~10 actions)
/// memory overhead.
pub struct MmapBacking<const MAX_ACTIONS: usize> {
    indexer: Indexer,
    mmap: MmapMut,
    path: Option<PathBuf>,
    populated_count: AtomicUsize,
    /// True when the indexer was just built (not loaded from disk) and needs
    /// to be written on the next commit. Once written, set to false so
    /// subsequent commits skip the expensive 226MB JSON serialization.
    indexer_needs_save: bool,
}

impl<const MAX_ACTIONS: usize> MmapBacking<MAX_ACTIONS> {
    const BUCKET_SIZE: usize = std::mem::size_of::<InfoState<MAX_ACTIONS>>();

    pub fn get(&self, key: &IStateKey) -> Option<InfoState<MAX_ACTIONS>> {
        let index: usize = self
            .indexer
            .index(key)
            .unwrap_or_else(|| panic!("failed to index {:?}", key));
        self.get_index(index)
    }

    fn get_index(&self, index: usize) -> Option<InfoState<MAX_ACTIONS>> {
        let start = index * Self::BUCKET_SIZE;

        if start + Self::BUCKET_SIZE > self.mmap.len() {
            return None;
        }

        let data = &self.mmap[start..start + Self::BUCKET_SIZE];

        if data.iter().all(|&x| x == 0) {
            return None;
        }

        let info = bytemuck::cast_slice::<u8, InfoState<MAX_ACTIONS>>(data)[0];
        Some(info)
    }

    pub fn put(&mut self, key: &IStateKey, value: &InfoState<MAX_ACTIONS>) {
        let index: usize = self
            .indexer
            .index(key)
            .unwrap_or_else(|| panic!("failed to index {:?}", key));
        let start = index * Self::BUCKET_SIZE;

        while start + Self::BUCKET_SIZE >= self.mmap.len() {
            let cur_len = self.mmap.len() / Self::BUCKET_SIZE;
            self.mmap.flush().expect("failed to flush mmap");
            self.mmap =
                get_mmap(self.path.as_deref(), cur_len + REMAP_INCREMENT, Self::BUCKET_SIZE)
                    .expect("failed to resize mmap");
            debug!("resized mmap");
        }

        let was_empty = self.mmap[start..start + Self::BUCKET_SIZE]
            .iter()
            .all(|&x| x == 0);

        let value = [*value];
        let data = bytemuck::cast_slice::<InfoState<MAX_ACTIONS>, u8>(&value);
        assert!(data.len() <= Self::BUCKET_SIZE);
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

        if self.indexer_needs_save {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(dir.join(INDEXER_NAME))?;
            let buf = serde_json::to_string(&self.indexer)?;
            file.write_all(buf.as_bytes())?;
            self.indexer_needs_save = false;
        }

        save_metadata(&dir, self.populated_count.load(Ordering::Relaxed))?;

        anyhow::Ok(())
    }

    pub fn len(&self) -> usize {
        self.populated_count.load(Ordering::Relaxed)
    }

    pub fn indexer_len(&self) -> usize {
        self.indexer.len()
    }
}

impl<const MAX_ACTIONS: usize> NodeStore<MAX_ACTIONS> {
    pub fn get(&self, key: &IStateKey) -> Option<InfoState<MAX_ACTIONS>> {
        match self {
            NodeStore::Mmap(m) => m.get(key),
            NodeStore::Hash(h) => h.get(key),
        }
    }

    pub fn put(&mut self, key: &IStateKey, value: &InfoState<MAX_ACTIONS>) {
        match self {
            NodeStore::Mmap(m) => m.put(key, value),
            NodeStore::Hash(h) => h.put(key, value),
        }
    }

    pub fn commit(&mut self) -> anyhow::Result<()> {
        match self {
            NodeStore::Mmap(m) => m.commit(),
            NodeStore::Hash(h) => h.commit(),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            NodeStore::Mmap(m) => m.len(),
            NodeStore::Hash(h) => h.len(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total addressable index size. For HashMap-backed stores there is no
    /// pre-allocated index, so this reports the same value as `len()`.
    pub fn indexer_len(&self) -> usize {
        match self {
            NodeStore::Mmap(m) => m.indexer_len(),
            NodeStore::Hash(h) => h.len(),
        }
    }
}

impl NodeStore<EUCHRE_MAX_ACTIONS> {
    /// len is the number of infostates to provision for
    pub fn new_euchre(path: Option<&Path>, max_cards_played: usize) -> anyhow::Result<Self> {
        let mmap = get_mmap(path, 20_000_000, MmapBacking::<EUCHRE_MAX_ACTIONS>::BUCKET_SIZE)
            .context("failed to create mmap")?;

        let path = path.map(|x| x.to_path_buf());
        let mut indexer_needs_save = false;
        let indexer = load_indexer(path.as_deref()).unwrap_or_else(|x| {
            warn!(
                "failed to load indexer from {:?}: {}",
                path.as_deref(),
                x
            );
            indexer_needs_save = true;
            Indexer::euchre(max_cards_played)
        });

        let populated_count = path
            .as_deref()
            .and_then(|p| load_metadata(p).ok())
            .unwrap_or_else(|| {
                count_populated(
                    &mmap,
                    &indexer,
                    MmapBacking::<EUCHRE_MAX_ACTIONS>::BUCKET_SIZE,
                    path.as_deref(),
                )
            });

        Ok(NodeStore::Mmap(MmapBacking {
            indexer,
            mmap,
            path,
            populated_count: AtomicUsize::new(populated_count),
            indexer_needs_save,
        }))
    }
}

impl NodeStore<KP_MAX_ACTIONS> {
    pub fn new_kp(path: Option<&Path>) -> anyhow::Result<Self> {
        if path.is_some() {
            bail!("serialization not supported for this game type")
        }

        let mmap = get_mmap(path, 1_000, MmapBacking::<KP_MAX_ACTIONS>::BUCKET_SIZE)?;

        let path = path.map(|x| x.to_path_buf());
        Ok(NodeStore::Mmap(MmapBacking {
            indexer: Indexer::kuhn_poker(),
            mmap,
            path,
            populated_count: AtomicUsize::new(0),
            indexer_needs_save: false,
        }))
    }
}

impl NodeStore<BLUFF_MAX_ACTIONS> {
    pub fn new_bluff_11(path: Option<&Path>) -> anyhow::Result<Self> {
        if path.is_some() {
            bail!("serialization not supported for this game type")
        }
        let mmap = get_mmap(path, 10_000, MmapBacking::<BLUFF_MAX_ACTIONS>::BUCKET_SIZE)?;

        let path = path.map(|x| x.to_path_buf());
        Ok(NodeStore::Mmap(MmapBacking {
            indexer: Indexer::bluff_11(),
            mmap,
            path,
            populated_count: AtomicUsize::new(0),
            indexer_needs_save: false,
        }))
    }
}

impl NodeStore<OH_MAX_ACTIONS> {
    /// Oh Hell with a HashMap-backed store. Full-game istate space is
    /// too large to enumerate up front for a perfect-hash index, but
    /// MCCFR only touches a small subset of istates per training run,
    /// so lazy population works well.
    ///
    /// When `path` is `Some` the constructor hydrates from disk if the
    /// file exists, and a later `commit()` will write the in-memory
    /// map back (MessagePack via `rmp-serde`). When `path` is `None`
    /// the store is purely in-memory.
    pub fn new_oh_hell(path: Option<&Path>, _n_tricks: usize) -> anyhow::Result<Self> {
        let owned = path.map(|p| p.to_path_buf());
        Ok(NodeStore::Hash(HashBacking::new(owned)?))
    }

    /// Oh Hell **bidding-only** with a disk-backed mmap + PHF store.
    /// The PHF is built over the canonical bidding-phase iso classes
    /// (the same set enumerated by
    /// [`games::gamestates::oh_hell::iterator::OhHellIsomorphicIStateIterator::bidding_only`]
    /// — Waugh-cross-checked to be exact), then each iso class maps
    /// to a fixed slot in the mmap, eliminating HashMap overhead per
    /// entry.
    ///
    /// `path` is the directory containing the indexer (`indexer`),
    /// the mmap file (`mmap`), and the populated-count file
    /// (`meta`). Pass `None` for an anonymous in-memory mmap (still
    /// PHF-indexed, but not persisted).
    /// OH **full-game** disk-backed mmap + PHF variant. The PHF is
    /// built over the canonical bidding + play-phase iso classes
    /// enumerated by
    /// [`games::gamestates::oh_hell::iterator::OhHellIsomorphicIStateIterator::full_game_via_waugh`]
    /// for `(num_players, n_tricks, max_cards_played)`.
    ///
    /// Same on-disk layout as the bidding-only variant: `indexer`,
    /// `mmap`, `meta`.
    pub fn new_oh_hell_full_game_mmap(
        path: Option<&Path>,
        num_players: usize,
        n_tricks: usize,
        max_cards_played: usize,
    ) -> anyhow::Result<Self> {
        let path = path.map(|x| x.to_path_buf());
        let mut indexer_needs_save = false;
        let indexer = load_indexer(path.as_deref()).unwrap_or_else(|x| {
            warn!(
                "failed to load OH full-game indexer from {:?}: {} — \
                 rebuilding (this may take a while for large configs)",
                path.as_deref(),
                x
            );
            indexer_needs_save = true;
            Indexer::oh_hell_full_game(num_players, n_tricks, max_cards_played)
        });

        let mmap = get_mmap(
            path.as_deref(),
            indexer.len(),
            MmapBacking::<OH_MAX_ACTIONS>::BUCKET_SIZE,
        )
        .context("failed to create OH full-game mmap")?;

        let populated_count = path
            .as_deref()
            .and_then(|p| load_metadata(p).ok())
            .unwrap_or_else(|| {
                count_populated(
                    &mmap,
                    &indexer,
                    MmapBacking::<OH_MAX_ACTIONS>::BUCKET_SIZE,
                    path.as_deref(),
                )
            });

        Ok(NodeStore::Mmap(MmapBacking {
            indexer,
            mmap,
            path,
            populated_count: AtomicUsize::new(populated_count),
            indexer_needs_save,
        }))
    }

}

fn get_mmap(dir: Option<&Path>, len: usize, bucket_size: usize) -> anyhow::Result<MmapMut> {
    let mmap = if let Some(dir) = dir {
        std::fs::create_dir_all(dir).context("failed to create directory")?;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(dir.join("mmap"))
            .context("failed to create mmap file")?;

        let target_size = (len * bucket_size) as u64;
        if file.metadata().context("failed to read file metadata")?.len() < target_size {
            file.set_len(target_size).context("failed to set length")?;
        }

        unsafe { MmapMut::map_mut(&file)? }
    } else {
        memmap2::MmapOptions::new()
            .len(len * bucket_size)
            .map_anon()?
    };

    mmap.advise(memmap2::Advice::Random)?;
    Ok(mmap)
}

/// Count the number of populated entries in an mmap by scanning every bucket.
/// Expensive for large mmaps (60GB+ for three_card_played). Prefer loading
/// the persisted count via `load_metadata` when available.
fn count_populated(mmap: &MmapMut, indexer: &Indexer, bucket_size: usize, dir: Option<&Path>) -> usize {
    warn!(
        "no persisted populated count found for {} — scanning {} mmap entries (this may take minutes for large weight files)",
        dir.map(|p| p.display().to_string()).unwrap_or_else(|| "<anonymous mmap>".to_string()),
        indexer.len()
    );
    let mut count = 0;
    for i in 0..indexer.len() {
        let start = i * bucket_size;
        if start + bucket_size <= mmap.len() {
            let data = &mmap[start..start + bucket_size];
            if !data.iter().all(|&x| x == 0) {
                count += 1;
            }
        }
    }
    count
}

fn save_metadata(dir: &Path, populated_count: usize) -> anyhow::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(dir.join(METADATA_NAME))?;
    write!(file, "{}", populated_count)?;
    Ok(())
}

fn load_metadata(dir: &Path) -> anyhow::Result<usize> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(dir.join(METADATA_NAME))?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    let count: usize = buf.trim().parse()?;
    Ok(count)
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
