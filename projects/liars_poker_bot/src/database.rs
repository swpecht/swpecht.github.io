use std::collections::HashMap;

use log::{debug, trace};
use sqlite::{Connection, State, Value};
use tempfile::{NamedTempFile, TempPath};

const INSERT_QUERY: &str = "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";
const GET_QUERY: &str = "SELECT * FROM nodes WHERE istate = :istate;";
const CACHE_SIZE: usize = 100000;

#[derive(Clone)]
pub enum Storage {
    Memory,
    Tempfile,
    Namedfile,
}

pub struct NodeStore {
    connection: Connection,
    storage: Storage,
    cache: HashMap<String, String>,
    // hold this so the temp file isn't detroyed
    _temp_file: Option<TempPath>,
}

/// NodeStore is a cache for istates and their associated game nodes.
///
/// It stores data in an in memory cache that is occasionally flushed to a sqlite database
impl NodeStore {
    pub fn new(storage: Storage) -> Self {
        let mut temp_file = None;
        let path = match storage {
            Storage::Memory => ":memory:".to_string(),
            Storage::Tempfile => {
                let f = NamedTempFile::new().unwrap();
                temp_file = Some(f.into_temp_path());
                temp_file.as_ref().unwrap().to_str().unwrap().to_string()
            }
            Storage::Namedfile => todo!(),
        };
        trace!("creating connection to sqlite at {}...", path);
        let connection = sqlite::open(path).unwrap();

        let query = "CREATE TABLE nodes (istate TEXT PRIMARY KEY, node TEXT);";
        connection.execute(query).unwrap();
        trace!("table created sucessfully");

        Self {
            connection,
            _temp_file: temp_file,
            storage,
            cache: HashMap::new(),
        }
    }

    pub fn get_node_mut(&mut self, istate: &str) -> Option<String> {
        if self.cache.contains_key(istate) {
            return self.cache.get(istate).cloned();
        }

        let mut statement = self.connection.prepare(GET_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.clone().into())][..])
            .unwrap();

        // Check if node found
        let r = statement.next();
        if r.unwrap() != State::Row {
            return None;
        };
        let node_ser = statement.read::<String, _>("node").unwrap();
        return Some(node_ser);
    }

    pub fn insert_node(&mut self, istate: String, s: String) -> Option<String> {
        if self.cache.len() > CACHE_SIZE {
            self.flush();
        }

        self.cache.insert(istate, s)
    }

    pub fn contains_node(&self, istate: &String) -> bool {
        if self.cache.contains_key(istate) {
            return true;
        }

        let mut statement = self.connection.prepare(GET_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.to_string().into())][..])
            .unwrap();

        let r = statement.next();

        return r.unwrap() == State::Row;
    }

    /// Writes all data in the cache to sqlite and clears the cache
    fn flush(&mut self) {
        debug!("flushing cache...");
        const BATCH_SIZE: usize = 1000;
        let mut i = 0;

        // Use a transaction for performance reasons
        self.connection.execute("BEGIN TRANSACTION;").unwrap();

        for (k, v) in self.cache.iter() {
            self.write_node(k.clone(), v.clone());

            if i % BATCH_SIZE == 0 {
                self.connection
                    .execute("COMMIT; BEGIN TRANSACTION;")
                    .unwrap();
            }

            i += 1;
        }

        self.connection.execute("COMMIT;").unwrap();
        debug!("flush complete");

        self.cache.clear();
    }

    fn write_node(&self, istate: String, s: String) -> Option<String> {
        let mut statement = self.connection.prepare(INSERT_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.into()), (":node", s.clone().into())][..])
            .unwrap();
        let r = statement.next();
        assert!(r.is_ok());

        return Some(s);
    }
}

impl Clone for NodeStore {
    fn clone(&self) -> Self {
        NodeStore::new(self.storage.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::{NodeStore, Storage};

    #[test]
    fn test_write_read_memory() {
        let mut store = NodeStore::new(Storage::Memory);
        let istate = "test".to_string();

        let s = "test node 1".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 1");

        let s = "test node 2".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 2");
    }

    #[test]
    fn test_write_read_tempfile() {
        let mut store = NodeStore::new(Storage::Tempfile);
        let istate = "test".to_string();

        let s = "test node 1".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 1");

        let s = "test node 2".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 2");
    }
}
