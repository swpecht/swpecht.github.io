use log::debug;
/// Implements a node storage backend for io_uring
///
/// Each istate can be though of as a tree of data
/// Require 2 parts for storage:
///     * Lookup table to figure out where a given chunk exists
///     * Storage of those chunk at the appropriate index
/// Simplest to just store each page as it's own file?
/// Pay some overhead on opening files, but should be minimal given we're constrained by
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashMap, path::PathBuf};
use tempfile::TempDir;
use tokio_uring::fs::File;

use crate::database::disk_backend::get_directory;

use super::{
    disk_backend::{get_path, DiskBackend},
    page::Page,
    Storage,
};

pub struct UringBackend {
    // Location of all files
    dir: PathBuf,
    // Hold a reference so the directory isn't deleted until this is dropped
    _temp: Option<TempDir>,
    buffer_size: usize,
}

impl UringBackend {
    pub fn new(storage: Storage) -> Self {
        // Uses a 64kb buffer, this is 4-5x faster than using a 4kb buffer for a 1M recrod write
        Self::new_with_buffer_size(storage, 65536)
    }

    pub fn new_with_buffer_size(storage: Storage, buffer_size: usize) -> Self {
        let (path, temp_dir) = get_directory(storage);

        debug!("setting up io_uring backend at: {}", path.display());
        Self {
            dir: path,
            _temp: temp_dir,
            buffer_size: buffer_size,
        }
    }
}

impl<T: Serialize + DeserializeOwned> DiskBackend<T> for UringBackend {
    fn write(&mut self, p: Page<T>) -> Result<(), &'static str> {
        let path = get_path(&p, &self.dir);
        let f = std::fs::File::create(path).unwrap();
        write_data(f, p.cache, self.buffer_size).unwrap();
        Ok(())
    }

    fn read(&self, mut p: Page<T>) -> Page<T> {
        let path = get_path(&p, &self.dir);
        let o_result = std::fs::File::open(path);

        if o_result.is_ok() {
            let f = o_result.unwrap();
            p.cache = read_data(f, self.buffer_size).unwrap();
        }

        return p;
    }

    fn write_sync(&mut self, p: Page<T>) -> Result<(), &'static str> {
        self.write(p)
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
    buffer_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio_uring::start(async {
        let file = File::from_std(file);

        let mut buf = vec![0; buffer_size];
        let mut pos = 0;
        let s = serde_json::to_string(&items).unwrap();
        let bytes = s.into_bytes();
        for c in bytes.chunks(buffer_size) {
            buf[..c.len()].copy_from_slice(c);
            if c.len() < buf.len() {
                for i in &mut buf[c.len()..] {
                    *i = ' ' as u8
                }
            }
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

pub fn read_data<T: DeserializeOwned>(
    file: std::fs::File,
    buffer_size: usize,
) -> Result<HashMap<String, T>, Box<dyn std::error::Error>> {
    tokio_uring::start(async {
        let file = File::from_std(file);
        let mut buf = vec![0; buffer_size];
        let mut pos = 0;
        let mut output = Vec::new();

        loop {
            let res;
            (res, buf) = file.read_at(buf, pos).await;
            let n = res?;
            if n == 0 {
                break; // end of file
            }
            pos += n as u64;
            output.append(&mut buf.clone());
        }

        let items = serde_json::from_slice(&output).unwrap();
        // Close the file
        file.close().await?;
        return Ok(items);
    })
}

#[cfg(test)]
mod tests {

    use rand::{distributions::Alphanumeric, Rng};

    use crate::database::{disk_backend::DiskBackend, page::Page, Storage};

    use super::UringBackend;

    #[test]
    fn test_io_uring_write_read_tempfile() {
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
        b.write(p.clone()).unwrap();

        let mut read: Page<Vec<char>> = Page::new("", &[]);
        read = b.read(read);

        assert_eq!(p.cache, read.cache);
    }
}