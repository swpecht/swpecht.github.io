use std::{
    collections::HashMap,
    sync::mpsc::{self, sync_channel, Sender, SyncSender},
    thread, time,
};

use log::{debug, info, warn};
use serde::{de::DeserializeOwned, Serialize};
use sqlite::{Connection, Error, State, Value};
use tempfile::{NamedTempFile, TempPath};

use super::{disk_backend::DiskBackend, page::Page, Storage};

pub struct SqliteBackend<T> {
    tx_write_page: SyncSender<Page<T>>,
    tx_exit: Sender<bool>,
    connection: Connection,
    // Hold a reference so the file isn't deleted
    _temp: Option<TempPath>,
}

impl<T: DeserializeOwned + Serialize + Send + 'static> SqliteBackend<T> {
    pub fn new(storage: Storage) -> Self {
        let (connection, temp_file, mut c2) = get_connection(storage.clone());

        let (tx_page, rx_page) = sync_channel::<Page<T>>(0);
        let (tx_exit, rx_exit) = mpsc::channel();

        thread::spawn(move || {
            debug!("starting IO thread");
            while rx_exit.try_recv() != Ok(true) {
                if let Ok(p) = rx_page.try_recv() {
                    write_data(&mut c2, p.cache);
                    debug!("commit finished for {}", p.istate);
                }

                let ten_millis = time::Duration::from_millis(10);
                thread::sleep(ten_millis)
            }
            debug!("exiting IO thread");
        });

        return Self {
            connection: connection,
            tx_write_page: tx_page,
            tx_exit: tx_exit,
            _temp: temp_file,
        };
    }

    /// Synchronous version of writing data for testing
    pub fn write_sync(&mut self, p: Page<T>) -> Result<(), &'static str> {
        write_data(&mut self.connection, p.cache);
        Ok(())
    }
}

impl<T: DeserializeOwned + Serialize> DiskBackend<T> for SqliteBackend<T> {
    fn write(&mut self, p: Page<T>) -> Result<(), &'static str> {
        self.tx_write_page.send(p).unwrap();
        Ok(())
    }

    fn read(&self, mut p: Page<T>) -> Page<T> {
        read_data(&self.connection, &p.istate, p.max_length, &mut p.cache);
        return p;
    }
}

impl<T> Clone for SqliteBackend<T> {
    fn clone(&self) -> Self {
        todo!();
    }
}

impl<T> Drop for SqliteBackend<T> {
    fn drop(&mut self) {
        // Shut down the IO thread
        self.tx_exit.send(true).unwrap();
    }
}

/// Writes the data to a database
///
/// This function uses a single transaction for thr write for performance reasons. Previous
/// implementations put a cap on the max transaction size. Removing that cap resulted in 80%+
/// speed up in benchmarks
pub fn write_data<T: Serialize>(c: &mut Connection, items: HashMap<String, T>) {
    const INSERT_QUERY: &str =
        "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";

    // Use a transaction for performance reasons
    c.execute("BEGIN TRANSACTION;").unwrap();

    for (k, v) in items.iter() {
        let s = serde_json::to_string(v).unwrap();
        let mut statement = c.prepare(INSERT_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", k.clone().into()), (":node", s.into())][..])
            .unwrap();
        let r = statement.next();
        if !r.is_ok() {
            panic!("{:?}", r);
        }
    }

    while c.execute("COMMIT;").is_err() {
        warn!("retrying write, database errord on commit");
    }
}

pub fn read_data<T>(c: &Connection, key: &String, max_len: usize, output: &mut HashMap<String, T>)
where
    T: DeserializeOwned,
{
    const LOAD_PAGE_QUERY: &str =
        "SELECT * FROM nodes WHERE istate LIKE :like AND LENGTH(istate) <= :maxlen;";

    // We are manually concatenating the '%' character in rust code to ensure that we are
    // performing the LIKE query against a string literal. This allows sqlite to use the
    // index for this query.
    let mut statement = c.prepare(LOAD_PAGE_QUERY).unwrap();
    let mut like_statement = key.clone();
    like_statement.push('%');
    statement
        .bind::<&[(_, Value)]>(
            &[
                (":like", like_statement.into()),
                (":maxlen", (max_len as i64).into()),
            ][..],
        )
        .unwrap();

    while let Ok(State::Row) = statement.next() {
        let node_ser = statement.read::<String, _>("node").unwrap();
        let istate = statement.read::<String, _>("istate").unwrap();
        let node = serde_json::from_str(&node_ser).unwrap();
        output.insert(istate, node);
    }
}

/// Returns a connection to sqlite database. Returns a second copy for use with
/// an IO thread.
pub fn get_connection(storage: Storage) -> (Connection, Option<TempPath>, Connection) {
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
    debug!("creating connection to sqlite at {}", path);
    let connection = sqlite::open(path.clone()).unwrap();

    // Turns off case insenstivity for like statements
    // this enables indexes to be used for queries.
    // https://stackoverflow.com/questions/8584499/sqlite-should-like-searchstr-use-an-index
    connection
        .execute("PRAGMA case_sensitive_like=OFF;")
        .unwrap();

    // Allows concurrent reading and writing
    // Preliminary testing shows this as a slight slowdown to operations
    // connection.execute("PRAGMA journal_mode=OFF;").unwrap();

    // connection.execute("PRAGMA synchronous=NORMAL;").unwrap();

    // We set `COLLATE NOCASE` to the istate filed to enable us to use an index
    let query =
        "CREATE TABLE IF NOT EXISTS nodes (istate TEXT PRIMARY KEY COLLATE NOCASE, node TEXT);";
    connection.execute(query).unwrap();

    // connection
    //     .execute("CREATE INDEX IF NOT EXISTS istate_idx ON nodes(istate COLLATE NOCASE);")
    //     .unwrap();

    return (connection, temp_file, sqlite::open(path).unwrap());
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rand::{distributions::Alphanumeric, Rng};

    use crate::database::{
        sqlite_backend::{get_connection, read_data, write_data},
        Storage,
    };

    #[test]
    fn test_sqlite_write_read_tempfile() {
        let (mut c, t, _) = get_connection(Storage::Tempfile);

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

        write_data(&mut c, data);
        let mut statement = c
            .prepare(
                "SELECT COUNT(*) FROM nodes WHERE istate LIKE '%' AND LENGTH(istate) <= 99999;",
            )
            .unwrap();
        statement.next().unwrap();
        assert_eq!(statement.read::<i64, _>(0).unwrap(), 1000);

        let mut output: HashMap<String, Vec<char>> = HashMap::new();
        read_data(&c, &"".to_string(), 99999, &mut output);

        assert_eq!(output.len(), 1000);

        drop(t);
    }
}
