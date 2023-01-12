use log::trace;
use sqlite::{Connection, State, Value};
use tempfile::{NamedTempFile, TempPath};

const INSERT_QUERY: &str = "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";
const GET_QUERY: &str = "SELECT * FROM nodes WHERE istate = :istate;";

#[derive(Clone)]
pub enum Storage {
    Memory,
    Tempfile,
    Namedfile,
}

pub struct NodeStore {
    connection: Connection,
    storage: Storage,
    temp_file: Option<TempPath>,
}

impl NodeStore {
    pub fn new(storage: Storage) -> Self {
        trace!("creating connection to sqlite...");

        let mut temp_file = None;
        let path = match storage {
            Storage::Memory => ":memory:".to_string(),
            Storage::Tempfile => {
                let f = NamedTempFile::new().unwrap();
                temp_file = Some(f.into_temp_path());
                temp_file.as_ref().unwrap().to_str().unwrap().to_string()
            }
            Storage::Namedfile => todo!(),
        };

        let connection = sqlite::open(path).unwrap();

        let query = "CREATE TABLE nodes (istate TEXT PRIMARY KEY, node TEXT);";
        connection.execute(query).unwrap();
        trace!("table created sucessfully");

        Self {
            connection,
            temp_file,
            storage,
        }
    }

    pub fn get_node_mut(&mut self, istate: &str) -> Option<String> {
        let mut statement = self.connection.prepare(GET_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.clone().into())][..])
            .unwrap();

        // Check if node found
        let r = statement.next();
        if r.unwrap() != State::Row {
            return None;
        };
        let node_ser = statement.read::<String, _>("node").unwrap();
        return Some(node_ser);
    }

    pub fn insert_node(&mut self, istate: String, s: String) -> Option<String> {
        let mut statement = self.connection.prepare(INSERT_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.into()), (":node", s.clone().into())][..])
            .unwrap();
        let r = statement.next();
        assert!(r.is_ok());

        return Some(s);
    }

    pub fn contains_node(&self, istate: &String) -> bool {
        let mut statement = self.connection.prepare(GET_QUERY).unwrap();
        statement
            .bind::<&[(_, Value)]>(&[(":istate", istate.to_string().into())][..])
            .unwrap();

        let r = statement.next();

        return r.unwrap() == State::Row;
    }
}

impl Clone for NodeStore {
    fn clone(&self) -> Self {
        NodeStore::new(self.storage.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::database::{NodeStore, Storage};

    #[test]
    fn test_write_read_memory() {
        let mut store = NodeStore::new(Storage::Memory);
        let istate = "test".to_string();

        let s = "test node 1".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 1");

        let s = "test node 2".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 2");
    }

    #[test]
    fn test_write_read_tempfile() {
        let mut store = NodeStore::new(Storage::Tempfile);
        let istate = "test".to_string();

        let s = "test node 1".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 1");

        let s = "test node 2".to_string();
        store.insert_node(istate.clone(), s);
        let r = store.get_node_mut(&istate);
        assert_eq!(r.unwrap(), "test node 2");
    }
}
