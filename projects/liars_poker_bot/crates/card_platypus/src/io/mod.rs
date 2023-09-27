use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use indicatif::ProgressBar;

/// Buffered file reader that displays a progress bar for the entire file
pub struct ProgressReader {
    reader: BufReader<File>,
    pb: ProgressBar,
}

impl ProgressReader {
    pub fn new(file: &Path) -> anyhow::Result<Self> {
        let f = File::open(file)?;
        let len = f.metadata()?.len();
        let pb = ProgressBar::new(len);
        let r = BufReader::new(f);
        Ok(Self { reader: r, pb })
    }

    pub fn finish(&mut self) {
        self.pb.finish_and_clear();
    }
}

impl Read for ProgressReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.reader.read(buf)?;
        self.pb.inc(n as u64);
        std::io::Result::Ok(n)
    }
}
