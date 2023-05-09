use rustc_hash::FxHashMap;
use serde::{de::DeserializeOwned, Serialize};

/// A vector like object that pages to disk after a certain number of elements have been loaded
pub struct DiskBackedVec<T> {
    /// pages currently in memory
    pages: FxHashMap<usize, Vec<T>>,
    page_size: usize,
    /// Maximum pages to keep in memory
    max_mem_pages: usize,
    len: usize,
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

        Self {
            pages: FxHashMap::default(),
            page_size: page_size,
            max_mem_pages: max_mem_pages,
            len: 0,
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
    }

    /// Gets the vec corresponding to the given index
    fn get_mem_vec(&mut self, idx: usize) -> &mut Vec<T> {
        let page_index = idx / self.page_size;
        let vec = self.pages.get_mut(&page_index);
        if vec.is_some() {
            return vec.unwrap();
        }

        todo!("loading from disk not yet implemented")
    }

    fn save_to_disk(&mut self) {
        todo!("saving to disk not yet implemented");
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
        todo!()
    }
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
