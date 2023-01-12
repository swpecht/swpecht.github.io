use cached::proc_macro::cached;
use cached::SizedCache;
use log::trace;
use sqlite::{Connection, State, Value};

use crate::cfragent::CFRNode;

const INSERT_QUERY: &str = "INSERT OR REPLACE INTO nodes (istate, node) VALUES (:istate, :node);";
const GET_QUERY: &str = "SELECT * FROM nodes WHERE istate = :istate";

pub fn get_connection() -> Connection {
    trace!("creating connection to sqlite...");

    let connection = sqlite::open(":memory:").unwrap();

    let query = "CREATE TABLE nodes (istate TEXT PRIMARY KEY, node TEXT);";
    connection.execute(query).unwrap();
    trace!("table created sucessfully");

    return connection;
}

// #[cached(
//     type = "SizedCache<String, Option<CFRNode>>",
//     create = "{ SizedCache::with_size(3000000) }",
//     convert = r#"{ format!("{}", istate) }"#
// )]
pub fn get_node_mut(istate: &str, connection: &Connection) -> Option<String> {
    let mut statement = connection.prepare(GET_QUERY).unwrap();
    statement
        .bind::<&[(_, Value)]>(&[(":istate", istate.clone().into())][..])
        .unwrap();

    // Check if node found
    if statement.next().unwrap() != State::Row {
        return None;
    };
    let node_ser = statement.read::<String, _>("node").unwrap();
    return Some(node_ser);
}

pub fn insert_node(istate: String, s: String, connection: &Connection) -> Option<String> {
    let mut statement = connection.prepare(INSERT_QUERY).unwrap();
    statement
        .bind::<&[(_, Value)]>(&[(":istate", istate.into()), (":node", s.clone().into())][..])
        .unwrap();

    let r = statement.next().unwrap();

    return Some(s);
}

pub fn contains_node(istate: &String, connection: &Connection) -> bool {
    let mut statement = connection.prepare(GET_QUERY).unwrap();
    statement
        .bind::<&[(_, Value)]>(&[(":istate", istate.to_string().into())][..])
        .unwrap();

    let r = statement.next();

    return r.unwrap() == State::Row;
}

#[cfg(test)]
mod tests {
    use super::{get_connection, get_node_mut, insert_node};

    #[test]
    fn test_write_read() {
        let con = &get_connection();
        let istate = "test".to_string();

        let s = "test node 1".to_string();
        insert_node(istate.clone(), s, con);
        let r = get_node_mut(&istate, con);
        assert_eq!(r.unwrap(), "test node 1");

        let s = "test node 2".to_string();
        insert_node(istate.clone(), s, con);
        let r = get_node_mut(&istate, con);
        assert_eq!(r.unwrap(), "test node 2");
    }
}
