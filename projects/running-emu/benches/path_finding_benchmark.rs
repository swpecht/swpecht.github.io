use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hecs::World;
use running_emu::{create_map, spatial::parse_map, AttackerAgent};

fn criterion_benchmark(c: &mut Criterion) {
    let map = create_map(1000);
    let mut world = World::new();
    parse_map(&mut world, &map);

    c.bench_function("find path 1000x1000", |b| {
        b.iter(|| find_path(black_box(&mut world)))
    });
}

fn find_path(world: &mut World) {
    let _agent = AttackerAgent::new(&world);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
