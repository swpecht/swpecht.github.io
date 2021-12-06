use criterion::{black_box, criterion_group, criterion_main, Criterion};
use running_emu::{create_map, FeatureFlags, PathingAlgorithm};

fn criterion_benchmark(c: &mut Criterion) {
    let map = "@..............
    .WWWWWWWWWWWWW.
    .W...........W.
    .W.WWWWWWWWW.W.
    .W.W.......W.W.
    .W.WWWWWWW.W.W.
    .W......GW.W.W.
    .WWWWWWWWW.W.W.
    ...........W...";

    let mut features = FeatureFlags::new();
    features.render = false;
    features.entity_spatial_cache = true;
    features.travel_matrix_for_goal_distance = true;
    features.pathing_algorithm = PathingAlgorithm::LpaStar;

    c.bench_function("find path spiral", |b| {
        b.iter(|| running_emu::run_sim_from_map(black_box(&map), features))
    });

    let large_map = create_map(20);
    c.bench_function("find path 20x20", |b| {
        b.iter(|| running_emu::run_sim_from_map(black_box(&large_map), features))
    });

    let large_map = create_map(100);
    c.bench_function("find path 100x100", |b| {
        b.iter(|| running_emu::run_sim_from_map(black_box(&large_map), features))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
