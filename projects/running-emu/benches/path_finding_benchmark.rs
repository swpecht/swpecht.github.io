use criterion::{black_box, criterion_group, criterion_main, Criterion};
use running_emu::{AttackerAgent, World, create_map, find_path_bfs};

fn criterion_benchmark(c: &mut Criterion) {
    let map = create_map(1000);


    let mut world = World::from_map(&map);
    

    c.bench_function("find path 1000x1000", |b| b.iter(|| find_path(black_box(&mut world))));
}

fn find_path(world: &mut World) {
    let mut agent = AttackerAgent::new(&world);
    find_path_bfs(world, &mut agent);
}


criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);