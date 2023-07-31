use std::{collections::HashMap, sync::Mutex};

use once_cell::sync::Lazy;

pub static mut COUNTERS: Mutex<Lazy<HashMap<String, usize>>> =
    Mutex::new(Lazy::new(HashMap::default));

pub fn increment_counter(name: &str) {
    unsafe {
        *COUNTERS
            .get_mut()
            .unwrap()
            .entry(name.to_string())
            .or_insert(0) += 1;
    }
}

pub fn read_counter(name: &str) -> usize {
    unsafe { *COUNTERS.lock().unwrap().get(name).unwrap_or(&0) }
}
