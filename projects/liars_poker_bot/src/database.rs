pub mod page;

use std::collections::{HashMap, VecDeque};

use log::{debug, trace};
use serde::Serialize;
use sqlite::{Connection, State, Value};
use tempfile::{NamedTempFile, TempPath};

use crate::database::page::Page;
use crate::{cfragent::CFRNode, database::page::EUCHRE_PAGE_TRIM};

const INSERT_QUERY: &str = "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";
const LOAD_PAGE_QUERY: &str = "SELECT * FROM nodes 
                                WHERE istate LIKE :istate
                                AND LENGTH(istate) <= :maxlen;";

#[derive(Clone)]
pub enum Storage {
    Memory,
    Tempfile,
    Namedfile(String),
}

struct NodeStoreStats {
    page_loads: HashMap<String, usize>,
}

impl NodeStoreStats {
    fn new() -> Self {
        Self {
            page_loads: HashMap::new(),
        }
    }
}

/// NodeStore is a cache for istates and their associated game nodes.
///
/// It stores an LRU of Pages. When a page is evicted, it's written to the database.
pub struct NodeStore {
    connection: Connection,
    storage: Storage,
    max_nodes: usize,
    pages: VecDeque<Page>,
    // hold this so the temp file isn't detroyed
    _temp_file: Option<TempPath>,
    // Keeps count of how often a given page is loaded into memory
    stats: NodeStoreStats,
}

impl NodeStore {
    pub fn new_with_pages(storage: Storage, max_nodes: usize) -> Self {
        let (connection, temp_file) = get_connection(storage.clone());

        Self {
            connection,
            _temp_file: temp_file,
            storage: storage.clone(),
            pages: VecDeque::new(),
            max_nodes: max_nodes,
            stats: NodeStoreStats::new(),
        }
    }

    pub fn new(storage: Storage) -> Self {
        NodeStore::new_with_pages(storage, 2000000)
    }

    pub fn get_node_mut(&mut self, istate: &str) -> Option<CFRNode> {
        for i in 0..self.pages.len() {
            if self.pages[i].contains(istate) {
                return self.pages[i].cache.get(istate).cloned();
            }
        }

        self.load_page(istate);

        return self.get_node_mut(istate);
    }

    pub fn insert_node(&mut self, istate: String, n: CFRNode) -> Option<CFRNode> {
        for i in 0..self.pages.len() {
            if self.pages[i].contains(&istate) {
                return self.pages[i].cache.insert(istate, n);
            }
        }

        self.load_page(&istate);

        return self.insert_node(istate, n);
    }

    pub fn contains_node(&mut self, istate: &String) -> bool {
        for i in 0..self.pages.len() {
            if self.pages[i].contains(istate) {
                return self.pages[i].cache.contains_key(istate);
            }
        }

        self.load_page(istate);

        return self.contains_node(istate);
    }

    /// Commits all data in the pages to sqlite
    fn commit(&mut self, page: Page) {
        debug!("commiting {} for page {}", page.cache.len(), page.istate);
        debug!("total pages: {}", self.pages.len());
        write_data(&self.connection, page.cache);
    }

    /// Loads the specified cursor into memory, flushing the previous cache
    fn load_page(&mut self, istate: &str) {
        trace!("page fault for:\t{}", istate);

        // find the page istate
        let mut p = Page::new(istate, EUCHRE_PAGE_TRIM);
        let max_len = p.max_length;

        {
            // We are manually concatenating the '%' character in rust code to ensure that we are
            // performing the LIKE query against a string literal. This allows sqlite to use the
            // index for this query.
            let mut statement = self.connection.prepare(LOAD_PAGE_QUERY).unwrap();
            statement
                .bind::<&[(_, Value)]>(
                    &[
                        (":istate", p.istate.clone().push('%').into()),
                        (":maxlen", (max_len as i64).into()),
                    ][..],
                )
                .unwrap();

            while let Ok(State::Row) = statement.next() {
                let node_ser = statement.read::<String, _>("node").unwrap();
                let istate = statement.read::<String, _>("istate").unwrap();
                let node = serde_json::from_str(&node_ser).unwrap();
                p.cache.insert(istate, node);
            }
        }

        let count = *self.stats.page_loads.get(&p.istate).unwrap_or(&0);
        self.stats.page_loads.insert(p.istate.clone(), count + 1);

        trace!("page loaded: {}\t{}", p.cache.len(), p.istate);
        debug!(
            "page '{}' loaded {} times ({} items)",
            p.istate,
            count + 1,
            p.cache.len()
        );
        trace!("{} pages loaded", self.pages.len());

        // Implement a FIFO cache
        self.pages.push_back(p);
        let mut cur_nodes = 0;
        for p in &self.pages {
            cur_nodes += p.cache.len();
        }

        if cur_nodes > self.max_nodes {
            let dropped = self.pages.pop_front();
            if let Some(dropped) = dropped {
                self.commit(dropped);
            }
        }
    }
}

impl Clone for NodeStore {
    fn clone(&self) -> Self {
        NodeStore::new_with_pages(self.storage.clone(), self.max_nodes)
    }
}

/// Writes the data to a database
///
/// This function uses a single transaction for thr write for performance reasons. Previous
/// implementations put a cap on the max transaction size. Removing that cap resulted in 80%+
/// speed up in benchmarks
pub fn write_data<T: Serialize>(c: &Connection, items: HashMap<String, T>) {
    // Use a transaction for performance reasons
    c.execute("BEGIN TRANSACTION;").unwrap();

    for (k, v) in items.iter() {
        let s = serde_json::to_string(v).unwrap();
        let mut statement = c.prepare(INSERT_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", k.clone().into()), (":node", s.into())][..])
            .unwrap();
        let r = statement.next();
        if !r.is_ok() {
            panic!("{:?}", r);
        }
    }

    c.execute("COMMIT;").unwrap();
}

pub fn get_connection(storage: Storage) -> (Connection, Option<TempPath>) {
    let mut temp_file = None;
    let path = match storage.clone() {
        Storage::Memory => ":memory:".to_string(),
        Storage::Tempfile => {
            let f = NamedTempFile::new().unwrap();
            temp_file = Some(f.into_temp_path());
            temp_file.as_ref().unwrap().to_str().unwrap().to_string()
        }
        Storage::Namedfile(x) => x,
    };
    debug!("creating connection to sqlite at {}", path);
    let connection = sqlite::open(path).unwrap();

    // Turns off case insenstivity for like statements
    // this enables indexes to be used for queries.
    // https://stackoverflow.com/questions/8584499/sqlite-should-like-searchstr-use-an-index
    connection
        .execute("PRAGMA case_sensitive_like=OFF;")
        .unwrap();

    // We set `COLLATE NOCASE` to the istate filed to enable us to use an index
    let query =
        "CREATE TABLE IF NOT EXISTS nodes (istate TEXT PRIMARY KEY COLLATE NOCASE, node TEXT);";
    connection.execute(query).unwrap();

    connection
        .execute("CREATE INDEX IF NOT EXISTS istate_idx ON nodes(istate COLLATE NOCASE);")
        .unwrap();

    return (connection, temp_file);
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

        let mut n = CFRNode::new(istate.clone(), &vec![0]);
        store.insert_node(istate.clone(), n.clone());
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, [0.0; 5]);

        n.regret_sum = [1.0; 5];
        store.insert_node(istate.clone(), n);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, [1.0; 5]);
    }

    #[test]
    fn test_write_page_read() {
        let mut store = NodeStore::new_with_pages(Storage::Tempfile, 1);
        let istate = "test".to_string();

        let mut n = CFRNode::new(istate.clone(), &vec![0]);
        store.insert_node(istate.clone(), n.clone());
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, [0.0; 5]);

        n.regret_sum = [1.0; 5];
        store.insert_node(istate.clone(), n);

        // force a page out
        store.get_node_mut("different page because it's much longer");
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, [1.0; 5]);
    }

    #[test]
    fn test_write_read_tempfile() {
        let mut store = NodeStore::new(Storage::Tempfile);
        let istate = "test".to_string();

        let mut n = CFRNode::new(istate.clone(), &vec![0]);
        store.insert_node(istate.clone(), n.clone());
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, [0.0; 5]);

        n.regret_sum = [1.0; 5];
        store.insert_node(istate.clone(), n);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap().regret_sum, [1.0; 5]);
    }
}
