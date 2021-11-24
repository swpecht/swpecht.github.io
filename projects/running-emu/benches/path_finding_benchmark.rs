use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::World;
use running_emu::{create_map, map::Map, find_path_bfs, AttackerAgent};

fn criterion_benchmark(c: &mut Criterion) {
    let str_map = create_map(1000);
    let mut world = World::new();

    let mut map = Map::new(&str_map, &mut world);

    c.bench_function("find path 1000x1000", |b| {
        b.iter(|| find_path(black_box(&mut map)))
    });
}

fn find_path(world: &mut Map) {
    let mut agent = AttackerAgent::new(&world);
    find_path_bfs(world, &mut agent);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
