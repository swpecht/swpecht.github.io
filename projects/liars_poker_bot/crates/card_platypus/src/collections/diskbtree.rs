use std::collections::HashMap;

use crossbeam_channel::{Receiver, RecvError, Select, Sender};

use crate::game::Action;

struct DiskStorage {
    receivers: Vec<Receiver<Request>>,
    transmitters: Vec<Sender<Response>>,
    data: HashMap<usize, usize>,
}

struct Request {
    key: Vec<Action>,
}

struct Response {}

fn recv_multiple<T>(rs: &[Receiver<T>]) -> Result<T, RecvError> {
    // Build a list of operations.
    let mut sel = Select::new();
    for r in rs {
        sel.recv(r);
    }

    // Complete the selected operation.
    let oper = sel.select();
    let index = oper.index();
    oper.recv(&rs[index])
}
