#![feature(test)]
extern crate renderer;
extern crate test;

use renderer::Universe;

#[bench]
fn universe_renders(b: &mut test::Bencher) {
    let mut universe = Universe::new();

    b.iter(|| {
        universe.render(0.0);
    });
}
