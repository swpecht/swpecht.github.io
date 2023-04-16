pub mod disk_backend;
pub mod file_backend;
pub mod memory_node_store;
pub mod page;
pub mod tune_page;

use std::collections::{HashMap, VecDeque};

use log::{debug, trace};

use crate::database::page::Page;
use crate::istate::IStateKey;
use crate::{cfragent::CFRNode, database::page::EUCHRE_PAGE_TRIM};

use self::disk_backend::DiskBackend;

#[derive(Clone)]
pub enum Storage {
    Memory,
    Temp,
    Named(String),
}

pub trait NodeStore {
    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode>;
    fn insert_node(&mut self, istate: IStateKey, n: CFRNode) -> Option<CFRNode>;
    fn contains_node(&mut self, istate: &IStateKey) -> bool;
}

struct NodeStoreStats {
    page_loads: HashMap<IStateKey, usize>,
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
pub struct FileNodeStore<T: DiskBackend<CFRNode>> {
    max_nodes: usize,
    pages: HashMap<IStateKey, Page<CFRNode>>,
    page_queue: VecDeque<IStateKey>,
    // Keeps count of how often a given page is loaded into memory
    stats: NodeStoreStats,
    backend: T,
}

impl<T: DiskBackend<CFRNode>> NodeStore for FileNodeStore<T> {
    fn get_node_mut(&mut self, istate: &IStateKey) -> Option<CFRNode> {
        let pgi = Page::<CFRNode>::get_page_key(istate, EUCHRE_PAGE_TRIM);
        if let Some(p) = self.pages.get_mut(&pgi) {
            return p.cache.get(istate).cloned();
        }

        self.load_page(istate);

        return self.get_node_mut(istate);
    }

    fn insert_node(&mut self, istate: IStateKey, n: CFRNode) -> Option<CFRNode> {
        let pgi = Page::<CFRNode>::get_page_key(&istate, EUCHRE_PAGE_TRIM);
        if let Some(p) = self.pages.get_mut(&pgi) {
            return p.cache.insert(istate, n);
        }

        self.load_page(&istate);

        return self.insert_node(istate, n);
    }

    fn contains_node(&mut self, istate: &IStateKey) -> bool {
        let pgi = Page::<CFRNode>::get_page_key(&istate, EUCHRE_PAGE_TRIM);
        if let Some(p) = self.pages.get_mut(&pgi) {
            return p.cache.contains_key(istate);
        }

        self.load_page(&istate);

        return self.contains_node(istate);
    }
}

impl<T: DiskBackend<CFRNode>> FileNodeStore<T> {
    pub fn new_with_pages(backend: T, max_nodes: usize) -> Self {
        Self {
            pages: HashMap::new(),
            max_nodes: max_nodes,
            stats: NodeStoreStats::new(),
            backend: backend,
            page_queue: VecDeque::new(),
        }
    }

    pub fn new(backend: T) -> Self {
        FileNodeStore::new_with_pages(backend, 5_000_000)
    }

    /// Commits all data in the pages to sqlite
    fn commit(&mut self, page: Page<CFRNode>) {
        debug!(
            "commiting {} for page {}",
            page.cache.len(),
            page.istate.to_string()
        );
        self.backend.write(page).unwrap();
    }

    /// Loads the specified cursor into memory, flushing the previous cache
    fn load_page(&mut self, istate: &IStateKey) {
        trace!("page fault for:\t{}", istate.to_string());

        // find the page istate
        let mut p = Page::new(istate, EUCHRE_PAGE_TRIM);

        debug!(
            "starting page load for: {} length {}",
            p.istate.to_string(),
            p.max_length
        );

        p = self.backend.read(p);

        let count = *self.stats.page_loads.get(&p.istate).unwrap_or(&0);
        self.stats.page_loads.insert(p.istate.clone(), count + 1);

        trace!("page loaded: {}\t{}", p.cache.len(), p.istate.to_string());
        debug!(
            "page '{}' loaded {} times ({} items)",
            p.istate.to_string(),
            count + 1,
            p.cache.len()
        );
        debug!("{} pages loaded", self.pages.len());

        // Implement a FIFO cache
        let pk = p.istate.clone();
        self.page_queue.push_back(pk.clone());
        self.pages.insert(pk, p);

        let mut cur_nodes = 0;
        for k in &self.page_queue {
            let p = self.pages.get(k).unwrap();
            cur_nodes += p.cache.len();
        }

        if cur_nodes > self.max_nodes {
            let dropped = self.page_queue.pop_front().unwrap();
            let p = self.pages.remove(&dropped);
            if let Some(p) = p {
                self.commit(p);
            }
        }
    }
}

impl<T: DiskBackend<CFRNode>> Clone for FileNodeStore<T> {
    fn clone(&self) -> Self {
        FileNodeStore::new_with_pages(self.backend.clone(), self.max_nodes)
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        cfragent::CFRNode,
        database::{file_backend::FileBackend, FileNodeStore, NodeStore, Storage},
        istate::IStateKey,
    };

    #[test]
    fn test_write_read_tempfile() {
        let mut store = FileNodeStore::new(FileBackend::new(Storage::Temp));
        let istate = IStateKey::new();

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
