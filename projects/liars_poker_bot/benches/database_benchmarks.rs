use criterion::{criterion_group, criterion_main, Criterion};
use liars_poker_bot::{
    cfragent::CFRNode,
    database::{NodeStore, Storage},
};
use rand::{distributions::Alphanumeric, Rng};

fn write_page() {
    let mut s = NodeStore::new_with_pages(Storage::Tempfile, 100000);

    for _ in 0..100001 {
        let istate: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(20)
            .map(char::from)
            .collect();
        let n = CFRNode::new(istate.clone(), &vec![0, 1, 2]);
        s.insert_node(istate, n);
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("write page", |b| b.iter(|| write_page()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
