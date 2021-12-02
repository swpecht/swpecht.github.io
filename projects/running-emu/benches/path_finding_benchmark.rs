use criterion::{black_box, criterion_group, criterion_main, Criterion};
use running_emu::create_map;

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

    c.bench_function("find path spiral", |b| {
        b.iter(|| running_emu::run_sim(black_box(&map), false))
    });

    let large_map = create_map(20);
    c.bench_function("find path 20x20", |b| {
        b.iter(|| running_emu::run_sim(black_box(&large_map), false))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
