use super::page::Page;

/// Trait to handle actually writing data to disk, including any multithreading that may be needed
pub trait DiskBackend: Clone {
    fn write(&mut self, p: Page) -> Result<(), &'static str>;
    fn read(&self, p: Page) -> Page;
}

/// Does no writing to disk
#[derive(Clone)]
struct NoOpBackend {}

impl DiskBackend for NoOpBackend {
    fn write(&mut self, _: Page) -> Result<(), &'static str> {
        return Ok(());
    }

    fn read(&self, p: Page) -> Page {
        return p;
    }
}
