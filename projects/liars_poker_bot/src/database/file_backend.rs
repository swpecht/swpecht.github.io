use std::{fs::File, path::PathBuf};

use serde::{de::DeserializeOwned, Serialize};
use tempfile::TempDir;

use super::disk_backend::DiskBackend;

pub struct FileBackend {
    // Location of all files
    dir: PathBuf,
    // Hold a reference so the directory isn't deleted until this is dropped
    _temp: Option<TempDir>,
    buffer_size: usize,
}

impl<T: Serialize + DeserializeOwned> DiskBackend<T> for FileBackend {
    fn write(&mut self, p: super::page::Page<T>) -> Result<(), &'static str> {
        todo!()
    }

    fn read(&self, p: super::page::Page<T>) -> super::page::Page<T> {
        todo!()
    }
}

impl Clone for FileBackend {
    fn clone(&self) -> Self {
        todo!()
    }
}
