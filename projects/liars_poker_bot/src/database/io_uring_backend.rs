/// Implements a node storage backend for io_uring
///
/// Each istate can be though of as a tree of data
/// Require 2 parts for storage:
///     * Lookup table to figure out where a given chunk exists
///     * Storage of those chunk at the appropriate index
/// Simplest to just store each page as it's own file?
/// Pay some overhead on opening files, but should be minimal given we're constrained by
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tempfile::{tempdir, NamedTempFile, TempDir, TempPath};
use tokio_uring::fs::File;

use super::{disk_backend::DiskBackend, page::Page, Storage};

pub struct UringBackend {
    // Location of all files
    dir: PathBuf,
    // Hold a reference so the directory isn't deleted until this is dropped
    _temp: Option<TempDir>,
}

impl UringBackend {
    pub fn new(storage: Storage) -> Self {
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

        Self {
            dir: path,
            _temp: temp_dir,
        }
    }
}

impl<T: Serialize> DiskBackend<T> for UringBackend {
    fn write(&mut self, p: Page<T>) -> Result<(), &'static str> {
        // Special case to handle the root node
        let name = match p.istate.as_str() {
            "" => "ROOT_NODE",
            _ => p.istate.as_str(),
        };

        let path = self.dir.join(name);
        let f = std::fs::File::create(path).unwrap();

        write_data(f, p.cache).unwrap();
        Ok(())
    }

    fn read(&self, p: Page<T>) -> Page<T> {
        todo!()
    }
}

impl Clone for UringBackend {
    fn clone(&self) -> Self {
        todo!();
    }
}

pub fn write_data<T: Serialize>(
    file: std::fs::File,
    items: HashMap<String, T>,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio_uring::start(async {
        let file = File::from_std(file);
        // Uses a 64kb buffer, this is 4-5x faster than using a 4kb buffer for a 1M recrod write
        let mut buf = vec![0; 65536];
        let mut pos = 0;
        let s = serde_json::to_string(&items).unwrap();
        let bytes = s.into_bytes();
        for c in bytes.chunks(65536) {
            buf[..c.len()].copy_from_slice(c);
            let res;
            (res, buf) = file.write_at(buf, pos).await;
            let n = res?;
            pos += n as u64;
        }

        // Close the file
        file.close().await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {

    use rand::{distributions::Alphanumeric, Rng};

    use crate::database::{disk_backend::DiskBackend, page::Page, Storage};

    use super::UringBackend;

    #[test]
    fn test_sqlite_write_read_tempfile() {
        let mut p = Page::new("", &[]);

        for _ in 0..1000 {
            let k: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(20)
                .map(char::from)
                .collect();
            let v: Vec<char> = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(20)
                .map(char::from)
                .collect();
            p.cache.insert(k, v);
        }

        let mut b = UringBackend::new(Storage::Temp);
        b.write(p).unwrap();

        todo!();
    }
}
