#![feature(test)]
extern crate renderer;
extern crate test;

use renderer::Universe;

#[bench]
fn universe_renders(b: &mut test::Bencher) {
    let mut universe = Universe::new();

    b.iter(|| {
        let mut i = 0.0;

        while i < 1.0 {
            universe.render(i);
            i += 0.1;
        }
    });
}
