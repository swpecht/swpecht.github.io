/// Implements a node storage backend for io_uring
///
/// Each istate can be though of as a tree of data
/// Require 2 parts for storage:
///     * Lookup table to figure out where a given chunk exists
///     * Storage of those chunk at the appropriate index
/// Simplest to just store each page as it's own file?
/// Pay some overhead on opening files, but should be minimal given we're constrained by
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};
use tempfile::{NamedTempFile, TempPath};
use tokio_uring::fs::File;

use super::{disk_backend::DiskBackend, page::Page, Storage};

pub struct UringBackend {
    // Location of all files
    dir: PathBuf,
    storage: Storage,
    // Hold a reference so the file isn't deleted
    _temp: Option<TempPath>,
}

impl UringBackend {
    pub fn new(storage: Storage) -> Self {
        let mut temp_file = None;
        let path = match storage.clone() {
            Storage::Memory => ":memory:".to_string(),
            Storage::Tempfile => {
                let f = NamedTempFile::new().unwrap();
                temp_file = Some(f.into_temp_path());
                temp_file.as_ref().unwrap().to_str().unwrap().to_string()
            }
            Storage::Namedfile(x) => x,
        };
        todo!()
    }
}

impl<T> DiskBackend<T> for UringBackend {
    fn write(&mut self, p: Page<T>) -> Result<(), &'static str> {
        todo!()
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

// pub fn get_file(storage: Storage) -> std::fs::File {
//     let file = match storage.clone() {
//         Storage::Memory => panic!("memory storage not supported to io_uring"),
//         Storage::Tempfile => {
//             let f = NamedTempFile::new().unwrap();
//             *f.as_file()
//         }
//         Storage::Namedfile(x) => fs::File::create(x).unwrap(),
//     };

//     return file;
// }

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, fs::File};

    use rand::{distributions::Alphanumeric, Rng};

    use crate::database::io_uring_backend::write_data;

    #[test]
    fn test_sqlite_write_read_tempfile() {
        let mut data: HashMap<String, Vec<char>> = HashMap::new();

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
            data.insert(k, v);
        }

        let file = File::create("/tmp/io_uring_test").unwrap();
        write_data(file, data).unwrap();
    }
}
