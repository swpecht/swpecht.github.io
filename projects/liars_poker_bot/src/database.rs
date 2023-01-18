use std::{
    collections::{HashMap, VecDeque},
    fmt::Debug,
};

use log::{debug, trace};
use sqlite::{Connection, State, Value};
use tempfile::{NamedTempFile, TempPath};

use crate::cfragent::CFRNode;

const INSERT_QUERY: &str = "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";
const LOAD_PAGE_QUERY: &str = "SELECT * FROM nodes 
                                WHERE istate LIKE :istate || '%'
                                AND LENGTH(istate) <= :maxlen;";
const PAGE_TRIM: usize = 50; // 3 levels of 2 chars each

#[derive(Clone)]
pub enum Storage {
    Memory,
    Tempfile,
    Namedfile,
}

/// Represents a collection of istates that are loaded into the cache.
///
/// It includes all children and parents of the `istate` it stores. The
/// `trim` variable determins how large the page is. It determins how many
/// istate characters must math
struct Page {
    istate: String,
    depth: usize,
    cache: HashMap<String, CFRNode>,
}

impl Page {
    fn new(istate: &str, depth: usize) -> Self {
        Self {
            istate: istate.to_string(),
            depth: depth,
            cache: HashMap::new(),
        }
    }

    fn contains(&self, istate: &str) -> bool {
        // Parent of the current page
        if istate.len() < self.istate.len() {
            return false;
        }

        // Different parent
        let target_parent = &istate[0..self.istate.len()];
        if target_parent != self.istate {
            return false;
        }

        return istate.len() <= self.istate.len() + self.depth;
    }
}

impl Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("istate", &self.istate)
            .field("depth", &self.depth)
            .finish()
    }
}

/// NodeStore is a cache for istates and their associated game nodes.
///
/// It stores an LRU of Pages. When a page is evicted, it's written to the database.
pub struct NodeStore {
    connection: Connection,
    storage: Storage,
    max_pages: usize,
    pages: VecDeque<Page>,
    // hold this so the temp file isn't detroyed
    _temp_file: Option<TempPath>,
    access_count: usize,
}

impl NodeStore {
    fn new_with_pages(storage: Storage, max_pages: usize) -> Self {
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
            pages: VecDeque::new(),
            access_count: 0,
            max_pages: max_pages,
        }
    }

    pub fn new(storage: Storage) -> Self {
        NodeStore::new_with_pages(storage, 1000)
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
        trace!("commiting {} for page {}", page.cache.len(), page.istate);
        const BATCH_SIZE: usize = 1000;
        let mut i = 0;

        // Use a transaction for performance reasons
        self.connection.execute("BEGIN TRANSACTION;").unwrap();

        for (k, v) in page.cache.iter() {
            let s = serde_json::to_string(v).unwrap();
            self.write_node(k.clone(), s);

            if i % BATCH_SIZE == 0 && i > 0 {
                self.connection
                    .execute("COMMIT; BEGIN TRANSACTION;")
                    .unwrap();
            }
            i += 1;
        }

        self.connection.execute("COMMIT;").unwrap();
    }

    fn write_node(&self, istate: String, s: String) {
        let mut statement = self.connection.prepare(INSERT_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.into()), (":node", s.into())][..])
            .unwrap();
        let r = statement.next();
        assert!(r.is_ok());
    }

    fn handle_db_access(&mut self) {
        self.access_count += 1;
        if self.access_count % 10000 == 0 {
            debug!("db read {} times", self.access_count);
        }
    }

    /// Loads the specified cursor into memory, flushing the previous cache
    fn load_page(&mut self, istate: &str) {
        trace!("page fault for: {}", istate);

        // find the page istate
        let len = istate.len();
        let excess = len % PAGE_TRIM;
        let page_istate = &istate[0..len - excess];
        let mut p = Page::new(page_istate, PAGE_TRIM);
        let max_len = page_istate.len() + PAGE_TRIM;

        {
            let mut statement = self.connection.prepare(LOAD_PAGE_QUERY).unwrap();
            statement
                .bind::<&[(_, Value)]>(
                    &[
                        (":istate", page_istate.to_string().into()),
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

        trace!("page loaded: {} items for {}", p.cache.len(), p.istate);

        // Implement a FIFO cache
        self.pages.push_back(p);
        if self.pages.len() > self.max_pages {
            let dropped = self.pages.pop_front();
            if let Some(dropped) = dropped {
                self.commit(dropped);
            }
        }

        self.handle_db_access();
    }
}

impl Clone for NodeStore {
    fn clone(&self) -> Self {
        NodeStore::new_with_pages(self.storage.clone(), self.max_pages)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        cfragent::CFRNode,
        database::{NodeStore, Storage},
    };

    use super::Page;

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
    fn test_write_page_read() {
        let mut store = NodeStore::new_with_pages(Storage::Tempfile, 1);
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

        // force a page out
        store.get_node_mut("different page because it's much longer");
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

    #[test]
    fn test_page_contains() {
        let p = Page::new("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD", 6);

        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JD"));
        assert!(p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDAD"));
        assert!(!p.contains("AC9HJHQHAHKH3C|AS10SKSAC|9CQHQDJS|JDADKCAH|"));
        assert!(!p.contains("XXXXXXXXXXXXXX|AS10SKSAC|9CQHQDJS|JDADKCAH|"));
    }
}
