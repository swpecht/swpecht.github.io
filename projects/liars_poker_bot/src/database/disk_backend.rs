use std::path::{Path, PathBuf};

use tempfile::{tempdir, TempDir};

use super::{page::Page, Storage};

/// Trait to handle actually writing data to disk, including any multithreading that may be needed
pub trait DiskBackend<T>: Clone {
    fn write(&mut self, p: Page<T>) -> Result<(), &'static str>;
    fn write_sync(&mut self, p: Page<T>) -> Result<(), &'static str>;
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

    fn write_sync(&mut self, _: Page<T>) -> Result<(), &'static str> {
        return Ok(());
    }
}

pub(super) fn get_path<T>(p: &Page<T>, dir: &PathBuf) -> PathBuf {
    // Special case to handle the root node
    let name = match p.istate.to_string().as_str() {
        "01" => "ROOT_NODE".to_owned(),
        _ => p.istate.to_string(),
    };

    let path = dir.join(name);
    return path;
}

pub(super) fn get_directory(storage: Storage) -> (PathBuf, Option<TempDir>) {
    let mut temp_dir = None;

    let path = match storage.clone() {
        Storage::Memory => panic!("memory backing not supported for io_uring"),
        Storage::Temp => {
            let dir = tempdir().unwrap();
            let path = dir.path().to_owned();
            temp_dir = Some(dir);
            path
        }
        Storage::Named(x) => Path::new(&x).to_owned(),
    };

    return (path, temp_dir);
}
