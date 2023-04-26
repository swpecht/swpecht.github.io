pub mod disk_backend;
pub mod file_backend;
pub mod memory_node_store;
pub mod node_tree;
pub mod page;
pub mod tune_page;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::istate::IStateKey;

#[derive(Clone)]
pub enum Storage {
    Memory,
    Temp,
    Named(String),
}

pub trait NodeStore<T> {
    fn get(&mut self, istate: &IStateKey) -> Option<Rc<RefCell<T>>>;
    fn insert_node(&mut self, istate: IStateKey, n: Rc<RefCell<T>>);
    fn contains_node(&mut self, istate: &IStateKey) -> bool;
}

struct _NodeStoreStats {
    page_loads: HashMap<IStateKey, usize>,
}

impl _NodeStoreStats {
    fn _new() -> Self {
        Self {
            page_loads: HashMap::new(),
        }
    }
}
