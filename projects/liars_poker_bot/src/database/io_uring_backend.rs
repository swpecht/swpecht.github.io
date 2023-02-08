use serde::Serialize;
use std::collections::HashMap;
use tokio_uring::fs::File;

pub fn write_data<T: Serialize>(
    items: HashMap<String, T>,
) -> Result<(), Box<dyn std::error::Error>> {
    tokio_uring::start(async {
        // Open a file
        let file = File::create("/tmp/io_uring_test").await?;
        let mut buf = vec![0; 4096];
        let mut pos = 0;
        let s = serde_json::to_string(&items).unwrap();
        let bytes = s.into_bytes();
        for c in bytes.chunks(4096) {
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
    use std::collections::HashMap;

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

        write_data(data);
    }
}
