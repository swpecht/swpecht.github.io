use std::{
    fs::File,
    io::{BufWriter, ErrorKind, Read, Write},
    path::PathBuf,
};

use log::debug;
use serde::{de::DeserializeOwned, Serialize};
use tempfile::TempDir;

use crate::database::disk_backend::get_directory;

use super::{
    disk_backend::{get_path, DiskBackend},
    Storage,
};

pub struct FileBackend {
    // Location of all files
    dir: PathBuf,
    // Hold a reference so the directory isn't deleted until this is dropped
    _temp: Option<TempDir>,
}

impl FileBackend {
    pub fn new(storage: Storage) -> Self {
        let (path, temp_dir) = get_directory(storage);
        debug!("setting up file backend at: {}", path.display());
        Self {
            dir: path,
            _temp: temp_dir,
        }
    }
}

impl<T: Serialize + DeserializeOwned> DiskBackend<T> for FileBackend {
    fn write(&mut self, p: super::page::Page<T>) -> Result<(), &'static str> {
        let path = get_path(&p, &self.dir);
        let f = File::create(path).unwrap();
        let f = BufWriter::new(f);

        serde_json::to_writer(f, &p.cache).unwrap();
        Ok(())
    }

    fn read(&self, mut p: super::page::Page<T>) -> super::page::Page<T> {
        let path = get_path(&p, &self.dir);
        let f = &mut File::open(&path);

        if f.is_err() && f.as_ref().err().unwrap().kind() == ErrorKind::NotFound {
            return p;
        }

        let f = f.as_mut().unwrap();

        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();

        p.cache = serde_json::from_slice(&buf).unwrap();

        return p;
    }

    fn write_sync(&mut self, p: super::page::Page<T>) -> Result<(), &'static str> {
        self.write(p)
    }
}

impl Clone for FileBackend {
    fn clone(&self) -> Self {
        todo!()
    }
}

#[cfg(test)]
mod tests {

    use rand::{distributions::Alphanumeric, Rng};

    use crate::database::{
        disk_backend::DiskBackend, file_backend::FileBackend, page::Page, Storage,
    };

    #[test]
    fn test_file_write_read_tempfile() {
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

        let mut b = FileBackend::new(Storage::Temp);
        b.write(p.clone()).unwrap();

        let mut read: Page<Vec<char>> = Page::new("", &[]);
        read = b.read(read);

        assert_eq!(p.cache, read.cache);
    }
}
