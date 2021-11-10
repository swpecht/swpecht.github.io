use criterion::{black_box, criterion_group, criterion_main, Criterion};
use running_emu::{World, find_path_bfs, create_map};

fn criterion_benchmark(c: &mut Criterion) {
    let map = create_map(1000);


    let world = World::from_map(&map);

    c.bench_function("find path 1000x1000", |b| b.iter(|| find_path_bfs(black_box(&world))));
}



criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);