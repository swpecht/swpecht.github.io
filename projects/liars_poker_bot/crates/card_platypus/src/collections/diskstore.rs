use std::{fs, path::Path};

use dashmap::{
    mapref::one::{Ref, RefMut},
    DashMap,
};
use heed::{types::SerdeBincode, Database, Env, EnvOpenOptions};
use itertools::Itertools;

use crate::{cfragent::cfres::InfoState, istate::IStateKey};

const MAX_CACHE_LEN: usize = 50_000_000;
const MAX_DB_SIZE_GB: usize = 100;

pub struct DiskStore {
    env: Option<Env>,
    db: Option<Database<heed::types::ByteSlice, SerdeBincode<InfoState>>>,
    cache: DashMap<IStateKey, InfoState>,
}

impl DiskStore {
    pub fn new(path: Option<&Path>) -> heed::Result<Self> {
        let cache = DashMap::new();
        if let Some(path) = path {
            fs::create_dir_all(path).unwrap();
            let env = EnvOpenOptions::new()
                .map_size(MAX_DB_SIZE_GB * 1024 * 1024 * 1024)
                .open(path)?;
            let db = env.create_database(None)?;
            Ok(Self {
                env: Some(env),
                db: Some(db),
                cache,
            })
        } else {
            Ok(Self {
                env: None,
                db: None,
                cache,
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

        if self.cache.len() > MAX_CACHE_LEN {
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

    /// Does a commit of the cache, could be expensive
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
}
