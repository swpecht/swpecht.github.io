use serde::{de::DeserializeOwned, Serialize};

use super::page::Page;

/// Trait to handle actually writing data to disk, including any multithreading that may be needed
pub trait DiskBackend<T>: Clone {
    fn write(&mut self, p: Page<T>) -> Result<(), &'static str>;
    fn read(&self, p: Page<T>) -> Page<T>;
}

/// Does no writing to disk
#[derive(Clone)]
pub struct NoOpBackend {}

impl NoOpBackend {
    pub fn new() -> Self {
        Self {}
    }
}

impl<T> DiskBackend<T> for NoOpBackend {
    fn write(&mut self, _: Page<T>) -> Result<(), &'static str> {
        return Ok(());
    }

    fn read(&self, p: Page<T>) -> Page<T> {
        return p;
    }
}
