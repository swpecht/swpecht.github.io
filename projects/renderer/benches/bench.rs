#![feature(test)]
extern crate test;

use renderer::{PixelBuffer, Universe};

#[bench]
fn universe_renders(b: &mut test::Bencher) {
    let mut universe = Universe::new();
    let mut pixel_buffer = PixelBuffer::new(&universe);

    b.iter(|| {
        universe.render(0.1, &mut pixel_buffer);
    });
}
