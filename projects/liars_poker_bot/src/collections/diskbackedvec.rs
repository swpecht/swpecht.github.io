use std::{collections::VecDeque, fs::File, io::BufWriter, path::PathBuf};

use rmp_serde::Serializer;
use rustc_hash::FxHashMap;
use serde::{de::DeserializeOwned, Serialize};
use tempfile::{tempdir, TempDir};

/// A vector like object that pages to disk after a certain number of elements have been loaded
pub struct DiskBackedVec<T> {
    /// pages currently in memory
    pages: FxHashMap<usize, Vec<T>>,
    page_size: usize,
    /// Maximum pages to keep in memory
    max_mem_pages: usize,
    len: usize,
    // A queue for pages, used to determine which page to evict if LIFO
    page_queue: VecDeque<usize>,
    dir: PathBuf,
    /// Hold a reference so the directory isn't deleted until this is dropped
    _temp: Option<TempDir>,
}

impl<T: Serialize + DeserializeOwned> DiskBackedVec<T> {
    pub fn new() -> Self {
        // defualt to 10M items in memory
        Self::with_sizes(10_000, 1000)
    }

    pub fn with_sizes(page_size: usize, max_mem_pages: usize) -> Self {
        if page_size < 2 {
            panic!("page_size must be >= 2");
        }

        let (dir, _temp) = get_cache_directory();

        Self {
            pages: FxHashMap::default(),
            page_size,
            max_mem_pages,
            len: 0,
            page_queue: VecDeque::new(),
            dir,
            _temp,
        }
    }

    pub fn len(&self) -> usize {
        return self.len;
    }

    /// Gets the item at the specified index, loading from disk if needed
    pub fn get(&mut self, idx: usize) -> &T {
        if idx >= self.len {
            panic!("index {} longer than length of {}", idx, self.len);
        }

        let in_page_idx = idx % self.page_size;
        let page = self.get_mem_vec(idx);
        return &page[in_page_idx];
    }

    pub fn push(&mut self, v: T) {
        let idx = self.len;
        self.len += 1;

        // need to create a new page
        if idx % self.page_size == 0 {
            self.create_page();
        }

        let page = self.get_mem_vec(idx);
        page.push(v);
    }

    /// Creates a new page at the end of the block
    fn create_page(&mut self) {
        // we should only do this at the start of a page
        assert_eq!(self.len % self.page_size, 1);

        if self.pages.len() >= self.max_mem_pages {
            self.save_to_disk();
        }

        let page_idx = self.len / self.page_size;
        let page = Vec::new();
        self.pages.insert(page_idx, page);
        self.page_queue.push_back(page_idx);
    }

    /// Gets the vec corresponding to the given index
    fn get_mem_vec(&mut self, idx: usize) -> &mut Vec<T> {
        let page_index = idx / self.page_size;
        self.ensure_page_loaded(page_index);
        return self.pages.get_mut(&page_index).unwrap();
    }

    fn ensure_page_loaded(&mut self, page_index: usize) {
        if !self.pages.contains_key(&page_index) {
            self.load_from_disk(page_index);
        }
    }

    fn load_from_disk(&mut self, page_index: usize) {
        if self.pages.len() >= self.max_mem_pages {
            self.save_to_disk();
        }

        let path = self.get_page_path(page_index);
        let f = &mut File::open(&path);

        let f = f.as_mut().unwrap();
        let page = rmp_serde::from_read(f).unwrap();

        self.page_queue.push_back(page_index);
        self.pages.insert(page_index, page);
    }

    /// Saves a single page to disk
    fn save_to_disk(&mut self) {
        let page_idx = self.page_queue.pop_front().unwrap();
        let page = self.pages.remove(&page_idx).unwrap();

        let path = self.get_page_path(page_idx);
        let f = File::create(path).unwrap();
        let f = BufWriter::new(f);

        page.serialize(&mut Serializer::new(f)).unwrap();
    }

    fn get_page_path(&self, page_idx: usize) -> PathBuf {
        return self.dir.join(page_idx.to_string());
    }
}

fn get_cache_directory() -> (PathBuf, Option<TempDir>) {
    let dir = tempdir().unwrap();
    let path = dir.path().to_owned();
    let temp_dir = Some(dir);
    return (path, temp_dir);
}

impl<T: Serialize + DeserializeOwned + Clone> IntoIterator for DiskBackedVec<T> {
    type Item = T;

    type IntoIter = DiskBackedVecInterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        DiskBackedVecInterator {
            vector: self,
            index: 0,
        }
    }
}

pub struct DiskBackedVecInterator<T> {
    vector: DiskBackedVec<T>,
    index: usize,
}

impl<'a, T: Serialize + DeserializeOwned + Clone> Iterator for DiskBackedVecInterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.vector.len() {
            return None;
        }

        let v = self.vector.get(self.index);
        self.index += 1;
        return Some(v.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::DiskBackedVec;

    #[test]
    fn test_disk_backed_vector() {
        let mut v = DiskBackedVec::new();
        assert_eq!(v.len(), 0);

        v.push(0);
        assert_eq!(v.len(), 1);
        assert_eq!(*v.get(0), 0);
    }

    #[test]
    fn test_disk_backed_vector_splitting() {
        let mut v = DiskBackedVec::with_sizes(2, 10);
        for i in 0..20 {
            v.push(i);
        }

        assert_eq!(v.len(), 20);
        for i in 0..20 {
            assert_eq!(*v.get(i), i);
        }
    }

    #[test]
    fn test_disk_backed_vector_caching() {
        let mut v = DiskBackedVec::with_sizes(2, 10);
        for i in 0..100 {
            v.push(i);
        }

        assert_eq!(v.len(), 100);
        for i in 0..100 {
            assert_eq!(*v.get(i), i);
        }
    }
}