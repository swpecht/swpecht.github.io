use std::collections::HashMap;

use log::{debug, trace};
use sqlite::{Connection, State, Value};
use tempfile::{NamedTempFile, TempPath};

use crate::cfragent::CFRNode;

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
    cache: HashMap<String, CFRNode>,
    // hold this so the temp file isn't detroyed
    _temp_file: Option<TempPath>,
    access_count: usize,
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
            access_count: 0,
        }
    }

    pub fn get_node_mut(&mut self, istate: &str) -> Option<CFRNode> {
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

        let node: Option<CFRNode> = Some(serde_json::from_str(&node_ser).unwrap());
        return node;
    }

    pub fn insert_node(&mut self, istate: String, n: CFRNode) -> Option<CFRNode> {
        if self.cache.len() > CACHE_SIZE {
            self.flush();
        }

        self.cache.insert(istate, n)
    }

    pub fn contains_node(&mut self, istate: &String) -> bool {
        if self.cache.contains_key(istate) {
            return true;
        }

        let result;
        {
            let mut statement = self.connection.prepare(GET_QUERY).unwrap();
            statement
                .bind::<&[(_, Value)]>(&[(":istate", istate.to_string().into())][..])
                .unwrap();

            let r = statement.next();
            result = r.unwrap() == State::Row;
        }

        self.handle_db_access();

        return result;
    }

    /// Writes all data in the cache to sqlite and clears the cache
    fn flush(&mut self) {
        debug!("flushing cache...");
        const BATCH_SIZE: usize = 1000;
        let mut i = 0;

        // Use a transaction for performance reasons
        self.connection.execute("BEGIN TRANSACTION;").unwrap();

        for (k, v) in self.cache.iter() {
            let s = serde_json::to_string(v).unwrap();
            self.write_node(k.clone(), s);

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

    fn handle_db_access(&mut self) {
        self.access_count += 1;
        if self.access_count % 10000 == 0 {
            debug!("db read {} times", self.access_count);
        }
    }
}

impl Clone for NodeStore {
    fn clone(&self) -> Self {
        NodeStore::new(self.storage.clone())
    }
}

impl Drop for NodeStore {
    fn drop(&mut self) {
        println!("dropping...")
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        cfragent::CFRNode,
        database::{NodeStore, Storage},
    };

    #[test]
    fn test_write_read_memory() {
        let mut store = NodeStore::new(Storage::Memory);
        let istate = "test".to_string();

        let n = CFRNode {
            info_set: istate.clone(),
            regret_sum: vec![0.0],
            strategy: vec![0.0],
            strategy_sum: vec![0.0],
        };
        store.insert_node(istate.clone(), n);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, vec![0.0]);

        let n = CFRNode {
            info_set: istate.clone(),
            regret_sum: vec![1.0],
            strategy: vec![0.0],
            strategy_sum: vec![0.0],
        };
        store.insert_node(istate.clone(), n);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, vec![1.0]);
    }

    #[test]
    fn test_write_read_tempfile() {
        let mut store = NodeStore::new(Storage::Tempfile);
        let istate = "test".to_string();

        let n = CFRNode {
            info_set: istate.clone(),
            regret_sum: vec![0.0],
            strategy: vec![0.0],
            strategy_sum: vec![0.0],
        };
        store.insert_node(istate.clone(), n);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, vec![0.0]);

        let n = CFRNode {
            info_set: istate.clone(),
            regret_sum: vec![1.0],
            strategy: vec![0.0],
            strategy_sum: vec![0.0],
        };
        store.insert_node(istate.clone(), n);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, vec![1.0]);
    }
}
