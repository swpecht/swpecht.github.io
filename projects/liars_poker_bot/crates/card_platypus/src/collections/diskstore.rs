use std::{fs, path::Path};

use dashmap::{
    mapref::one::{Ref, RefMut},
    DashMap,
};
use heed::{flags::Flags, types::SerdeBincode, Database, Env, EnvOpenOptions};
use itertools::Itertools;

use crate::{algorithms::cfres::InfoState, istate::IStateKey};

const DEFAULT_CACHE_LEN: usize = 30_000_000;
const MAX_DB_SIZE_GB: usize = 200;

pub struct DiskStore {
    env: Option<Env>,
    db: Option<Database<heed::types::ByteSlice, SerdeBincode<InfoState>>>,
    cache: DashMap<IStateKey, InfoState>,
    max_cache_len: usize,
}

impl DiskStore {
    pub fn new(path: Option<&Path>) -> heed::Result<Self> {
        let cache = DashMap::new();
        if let Some(path) = path {
            fs::create_dir_all(path).unwrap();

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
                // Uses asynchronous flushes to disk, a system crash can corrupt the databse or lose data
                // env_builder.flag(Flags::MdbMapAsync);
                // todo, explore other flags
            }
            env_builder.map_size(MAX_DB_SIZE_GB * 1024 * 1024 * 1024);

            let env = env_builder.open(path)?;
            // need to open rather than create for WriteMap to work
            let db = env.open_database(None).unwrap().unwrap();
            // let db = env.create_database(None).unwrap();
            Ok(Self {
                env: Some(env),
                db: Some(db),
                cache,
                max_cache_len: DEFAULT_CACHE_LEN,
            })
        } else {
            Ok(Self {
                env: None,
                db: None,
                cache,
                max_cache_len: DEFAULT_CACHE_LEN,
            })
        }
    }

    pub fn get_or_create_mut(
        &self,
        key: &IStateKey,
        default: InfoState,
    ) -> RefMut<IStateKey, InfoState> {
        let cache_result = self.cache.get_mut(key);
        if let Some(cr) = cache_result {
            return cr;
        }

        let disk_value = self.get_from_disk(*key);
        let value = if let Some(dv) = disk_value {
            self.cache.entry(*key).or_insert(dv)
        } else {
            self.cache.entry(*key).or_insert(default)
        };

        value
    }

    pub fn get(&self, key: &IStateKey) -> Option<Ref<IStateKey, InfoState>> {
        let cache_result = self.cache.get(key);
        if cache_result.is_some() {
            return cache_result;
        }

        let disk_value = self.get_from_disk(*key);
        if let Some(dv) = disk_value {
            let value = self.cache.entry(*key).or_insert(dv);
            return Some(value.downgrade());
        } else {
            None
        }
    }

    pub fn put(&self, key: IStateKey, value: InfoState) {
        self.cache.insert(key, value);

        if self.cache.len() > self.max_cache_len {
            self.commit();
        }
    }

    /// Writes all cached values to disk and drains the cache
    pub fn commit(&self) {
        if self.env.is_none() || self.db.is_none() {
            return;
        }

        let mut wtxn = self.env.as_ref().unwrap().write_txn().unwrap();

        for e in self.cache.iter() {
            let key = e.key().as_slice().iter().map(|x| x.0).collect_vec();
            self.db
                .unwrap()
                .put(&mut wtxn, key.as_slice(), e.value())
                .unwrap();
        }

        self.cache.clear();
        wtxn.commit().unwrap();
    }

    fn get_from_disk(&self, key: IStateKey) -> Option<InfoState> {
        if self.env.is_none() || self.db.is_none() {
            return None;
        }

        let rtxn = self.env.as_ref().unwrap().read_txn().unwrap();
        let k = key.as_slice().iter().map(|x| x.0).collect_vec();
        self.db.unwrap().get(&rtxn, k.as_slice()).unwrap()
    }

    /// Returns the len of commited items, doesn't include item in the cache
    pub fn len(&self) -> usize {
        if self.db.is_none() || self.env.is_none() {
            self.cache.len()
        } else {
            let rtxn = self.env.as_ref().unwrap().read_txn().unwrap();
            self.db.unwrap().len(&rtxn).unwrap() as usize
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn set_cache_len(&mut self, len: usize) {
        self.max_cache_len = len;
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::DiskStore;

    #[test]
    fn test_open_database() {
        let path = Path::new("/tmp/card_platypus").join("bytemuck.mdb");
        let mut _x = DiskStore::new(Some(&path)).unwrap();
    }
}
