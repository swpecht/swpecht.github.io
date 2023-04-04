pub mod disk_backend;
pub mod file_backend;
pub mod io_uring_backend;
pub mod memory_backend;
pub mod page;

use std::collections::{HashMap, VecDeque};

use log::{debug, trace};

use crate::database::page::Page;
use crate::{cfragent::CFRNode, database::page::EUCHRE_PAGE_TRIM};

use self::disk_backend::DiskBackend;

#[derive(Clone)]
pub enum Storage {
    Memory,
    Temp,
    Named(String),
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
/// It stores an FIFO queue of Pages. When a page is evicted, it's written by the diskbackend.
pub struct NodeStore<T: DiskBackend<CFRNode>> {
    max_nodes: usize,
    pages: VecDeque<Page<CFRNode>>,
    // Keeps count of how often a given page is loaded into memory
    stats: NodeStoreStats,
    backend: T,
}

impl<T: DiskBackend<CFRNode>> NodeStore<T> {
    pub fn new_with_pages(backend: T, max_nodes: usize) -> Self {
        Self {
            pages: VecDeque::new(),
            max_nodes: max_nodes,
            stats: NodeStoreStats::new(),
            backend: backend,
        }
    }

    pub fn new(backend: T) -> Self {
        NodeStore::new_with_pages(backend, 2000000)
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
    fn commit(&mut self, page: Page<CFRNode>) {
        debug!("commiting {} for page {}", page.cache.len(), page.istate);
        self.backend.write(page).unwrap();
    }

    /// Loads the specified cursor into memory, flushing the previous cache
    fn load_page(&mut self, istate: &str) {
        trace!("page fault for:\t{}", istate);

        // find the page istate
        let mut p = Page::new(istate, EUCHRE_PAGE_TRIM);

        debug!(
            "starting page load for: {} length {}",
            p.istate, p.max_length
        );

        p = self.backend.read(p);

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

impl<T: DiskBackend<CFRNode>> Clone for NodeStore<T> {
    fn clone(&self) -> Self {
        NodeStore::new_with_pages(self.backend.clone(), self.max_nodes)
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        cfragent::CFRNode,
        database::{file_backend::FileBackend, NodeStore, Storage},
    };

    #[test]
    fn test_write_read_tempfile() {
        let mut store = NodeStore::new(FileBackend::new(Storage::Temp));
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
