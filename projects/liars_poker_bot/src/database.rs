pub mod page;
pub mod sqlite_backend;

use std::collections::{HashMap, VecDeque};

use std::sync::mpsc::{self, Sender};
use std::sync::mpsc::{sync_channel, SyncSender};

use std::thread::{self};
use std::time;

use log::{debug, trace};
use sqlite::Connection;
use tempfile::TempPath;

use crate::database::page::Page;
use crate::{cfragent::CFRNode, database::page::EUCHRE_PAGE_TRIM};

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
/// Writing pages is done on a separate thread.
pub struct NodeStore {
    connection: Connection,
    storage: Storage,
    max_nodes: usize,
    pages: VecDeque<Page>,
    // hold this so the temp file isn't detroyed
    _temp_file: Option<TempPath>,
    // Keeps count of how often a given page is loaded into memory
    stats: NodeStoreStats,
    tx_write_page: SyncSender<Page>,
    tx_exit: Sender<bool>,
}

impl NodeStore {
    pub fn new_with_pages(storage: Storage, max_nodes: usize) -> Self {
        let (connection, temp_file, mut c2) = sqlite_backend::get_connection(storage.clone());

        let (tx_page, rx_page) = sync_channel::<Page>(0);
        let (tx_exit, rx_exit) = mpsc::channel();

        thread::spawn(move || {
            debug!("starting IO thread");
            while rx_exit.try_recv() != Ok(true) {
                if let Ok(p) = rx_page.try_recv() {
                    sqlite_backend::write_data(&mut c2, p.cache);
                    debug!("commit finished for {}", p.istate);
                }

                let ten_millis = time::Duration::from_millis(10);
                thread::sleep(ten_millis)
            }
            debug!("exiting IO thread");
        });

        Self {
            connection,
            _temp_file: temp_file,
            storage: storage.clone(),
            pages: VecDeque::new(),
            max_nodes: max_nodes,
            stats: NodeStoreStats::new(),
            tx_write_page: tx_page,
            tx_exit: tx_exit,
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
        self.tx_write_page.send(page).unwrap();
    }

    /// Loads the specified cursor into memory, flushing the previous cache
    fn load_page(&mut self, istate: &str) {
        trace!("page fault for:\t{}", istate);

        // find the page istate
        let mut p = Page::new(istate, EUCHRE_PAGE_TRIM);
        let max_len = p.max_length;

        debug!(
            "starting page load for: {} length {}",
            p.istate, p.max_length
        );

        sqlite_backend::read_data(&self.connection, &p.istate, max_len, &mut p.cache);

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

impl Drop for NodeStore {
    fn drop(&mut self) {
        // Shut down the IO thread
        self.tx_exit.send(true).unwrap();
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
