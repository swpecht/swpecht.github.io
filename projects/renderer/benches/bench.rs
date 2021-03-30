use criterion::{black_box, criterion_group, criterion_main, Criterion};

use renderer::{PixelBuffer, Universe};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut universe = Universe::new();
    let mut pixel_buffer = PixelBuffer::new(&universe);

    c.bench_function("render", |b| {
        b.iter(|| universe.render(black_box(0.1), &mut pixel_buffer))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
