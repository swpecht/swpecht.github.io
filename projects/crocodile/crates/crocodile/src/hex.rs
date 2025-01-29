// Generated code -- CC0 -- No Rights Reserved -- http://www.redblobgames.com/grids/hexagons/
// https://www.redblobgames.com/grids/hexagons/implementation.html

#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(unused_mut)] // TODO: remove this by analyzing output code

use std::cmp::max;
use std::f64::consts::PI;

use bevy::math::{vec2, Vec2};

use crate::TILE_SIZE;
const SQRT_3: f64 = 1.73205080756888;

pub fn coords_to_pixel(row: usize, col: usize) -> Vec2 {
    let layout = Layout {
        orientation: layout_pointy,
        size: Point {
            x: TILE_SIZE as f64 / 2.0,
            y: TILE_SIZE as f64 / 2.0,
        },
        origin: Point { x: 0.0, y: 0.0 },
    };

    let offset = OffsetCoord {
        col: col as i32,
        row: row as i32,
    };
    let hex = roffset_to_cube(ODD, offset);
    // let hex = Hex::new(r, c);
    let pixel = hex_to_pixel(layout, hex);
    bevy::math::vec2(pixel.x as f32, pixel.y as f32)
}

pub fn pixel_to_coords(loc: Vec2) -> Vec2 {
    let layout = Layout {
        orientation: layout_pointy,
        size: Point {
            x: TILE_SIZE as f64 / 2.0,
            y: TILE_SIZE as f64 / 2.0,
        },
        origin: Point { x: 0.0, y: 0.0 },
    };
    let hex = pixel_to_hex(
        layout,
        Point {
            x: loc.x as f64,
            y: loc.y as f64,
        },
    );
    let hex = hex_round(hex);
    let coords = roffset_from_cube(ODD, hex);
    vec2(coords.col as f32, coords.row as f32)
}

pub fn vertices(loc: Vec2) -> Vec<Vec2> {
    let layout = Layout {
        orientation: layout_pointy,
        size: Point {
            x: TILE_SIZE as f64 / 2.0,
            y: TILE_SIZE as f64 / 2.0,
        },
        origin: Point { x: 0.0, y: 0.0 },
    };
    let hex = pixel_to_hex(
        layout,
        Point {
            x: loc.x as f64,
            y: loc.y as f64,
        },
    );
    let hex = hex_round(hex);
    polygon_corners(layout, hex)
        .into_iter()
        .map(|p| vec2(p.x as f32, p.y as f32))
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Hex {
    q: i32,
    r: i32,
    s: i32,
}

impl Hex {
    pub fn new(r: usize, c: usize) -> Hex {
        Hex {
            q: c as i32,
            r: r as i32,
            s: -(c as i32) - r as i32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FractionalHex {
    pub q: f64,
    pub r: f64,
    pub s: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct OffsetCoord {
    pub col: i32,
    pub row: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct DoubledCoord {
    pub col: i32,
    pub row: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Orientation {
    pub f0: f64,
    pub f1: f64,
    pub f2: f64,
    pub f3: f64,
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub b3: f64,
    pub start_angle: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub orientation: Orientation,
    pub size: Point,
    pub origin: Point,
}

pub fn hex_add(a: Hex, b: Hex) -> Hex {
    Hex {
        q: a.q + b.q,
        r: a.r + b.r,
        s: a.s + b.s,
    }
}

pub fn hex_subtract(a: Hex, b: Hex) -> Hex {
    Hex {
        q: a.q - b.q,
        r: a.r - b.r,
        s: a.s - b.s,
    }
}

pub fn hex_scale(a: Hex, k: i32) -> Hex {
    Hex {
        q: a.q * k,
        r: a.r * k,
        s: a.s * k,
    }
}

pub fn hex_rotate_left(a: Hex) -> Hex {
    Hex {
        q: -a.s,
        r: -a.q,
        s: -a.r,
    }
}

pub fn hex_rotate_right(a: Hex) -> Hex {
    Hex {
        q: -a.r,
        r: -a.s,
        s: -a.q,
    }
}

static hex_directions: [Hex; 6] = [
    Hex { q: 1, r: 0, s: -1 },
    Hex { q: 1, r: -1, s: 0 },
    Hex { q: 0, r: -1, s: 1 },
    Hex { q: -1, r: 0, s: 1 },
    Hex { q: -1, r: 1, s: 0 },
    Hex { q: 0, r: 1, s: -1 },
];
pub fn hex_direction(direction: i32) -> Hex {
    hex_directions[direction as usize]
}

pub fn hex_neighbor(hex: Hex, direction: i32) -> Hex {
    hex_add(hex, hex_direction(direction))
}

static hex_diagonals: [Hex; 6] = [
    Hex { q: 2, r: -1, s: -1 },
    Hex { q: 1, r: -2, s: 1 },
    Hex { q: -1, r: -1, s: 2 },
    Hex { q: -2, r: 1, s: 1 },
    Hex { q: -1, r: 2, s: -1 },
    Hex { q: 1, r: 1, s: -2 },
];
pub fn hex_diagonal_neighbor(hex: Hex, direction: i32) -> Hex {
    hex_add(hex, hex_diagonals[direction as usize])
}

pub fn hex_length(hex: Hex) -> i32 {
    (hex.q.abs() + hex.r.abs() + hex.s.abs()) / 2
}

pub fn hex_distance(a: Hex, b: Hex) -> i32 {
    hex_length(hex_subtract(a, b))
}

pub fn hex_round(h: FractionalHex) -> Hex {
    let mut qi: i32 = h.q.round() as i32;
    let mut ri: i32 = h.r.round() as i32;
    let mut si: i32 = h.s.round() as i32;
    let mut q_diff: f64 = (qi as f64 - h.q).abs();
    let mut r_diff: f64 = (ri as f64 - h.r).abs();
    let mut s_diff: f64 = (si as f64 - h.s).abs();
    if q_diff > r_diff && q_diff > s_diff {
        qi = -ri - si;
    } else if r_diff > s_diff {
        ri = -qi - si;
    } else {
        si = -qi - ri;
    }
    Hex {
        q: qi,
        r: ri,
        s: si,
    }
}

pub fn hex_lerp(a: FractionalHex, b: FractionalHex, t: f64) -> FractionalHex {
    FractionalHex {
        q: a.q * (1.0 - t) + b.q * t,
        r: a.r * (1.0 - t) + b.r * t,
        s: a.s * (1.0 - t) + b.s * t,
    }
}

pub fn hex_linedraw(a: Hex, b: Hex) -> Vec<Hex> {
    let mut N: i32 = hex_distance(a, b);
    let mut a_nudge: FractionalHex = FractionalHex {
        q: a.q as f64 + 1e-06,
        r: a.r as f64 + 1e-06,
        s: a.s as f64 - 2e-06,
    };
    let mut b_nudge: FractionalHex = FractionalHex {
        q: b.q as f64 + 1e-06,
        r: b.r as f64 + 1e-06,
        s: b.s as f64 - 2e-06,
    };
    let mut results: Vec<Hex> = vec![];
    let mut step: f64 = 1.0 / max(N, 1) as f64;
    for i in 0..=N {
        results.push(hex_round(hex_lerp(a_nudge, b_nudge, step * i as f64)));
    }
    results
}

pub const EVEN: i32 = 1;
pub const ODD: i32 = -1;
pub fn qoffset_from_cube(offset: i32, h: Hex) -> OffsetCoord {
    let mut col: i32 = h.q;
    let mut row: i32 = h.r + (h.q + offset * (h.q & 1)) / 2;
    if offset != EVEN && offset != ODD {
        panic!("offset must be EVEN (+1) or ODD (-1)");
    }
    OffsetCoord { col, row }
}

pub fn qoffset_to_cube(offset: i32, h: OffsetCoord) -> Hex {
    let mut q: i32 = h.col;
    let mut r: i32 = h.row - (h.col + offset * (h.col & 1)) / 2;
    let mut s: i32 = -q - r;
    if offset != EVEN && offset != ODD {
        panic!("offset must be EVEN (+1) or ODD (-1)");
    }
    Hex { q, r, s }
}

pub fn roffset_from_cube(offset: i32, h: Hex) -> OffsetCoord {
    let mut col: i32 = h.q + (h.r + offset * (h.r & 1)) / 2;
    let mut row: i32 = h.r;
    if offset != EVEN && offset != ODD {
        panic!("offset must be EVEN (+1) or ODD (-1)");
    }
    OffsetCoord { col, row }
}

pub fn roffset_to_cube(offset: i32, h: OffsetCoord) -> Hex {
    let mut q: i32 = h.col - (h.row + offset * (h.row & 1)) / 2;
    let mut r: i32 = h.row;
    let mut s: i32 = -q - r;
    if offset != EVEN && offset != ODD {
        panic!("offset must be EVEN (+1) or ODD (-1)");
    }
    Hex { q, r, s }
}

pub fn qdoubled_from_cube(h: Hex) -> DoubledCoord {
    let mut col: i32 = h.q;
    let mut row: i32 = 2 * h.r + h.q;
    DoubledCoord { col, row }
}

pub fn qdoubled_to_cube(h: DoubledCoord) -> Hex {
    let mut q: i32 = h.col;
    let mut r: i32 = (h.row - h.col) / 2;
    let mut s: i32 = -q - r;
    Hex { q, r, s }
}

pub fn rdoubled_from_cube(h: Hex) -> DoubledCoord {
    let mut col: i32 = 2 * h.q + h.r;
    let mut row: i32 = h.r;
    DoubledCoord { col, row }
}

pub fn rdoubled_to_cube(h: DoubledCoord) -> Hex {
    let mut q: i32 = (h.col - h.row) / 2;
    let mut r: i32 = h.row;
    let mut s: i32 = -q - r;
    Hex { q, r, s }
}

pub const layout_pointy: Orientation = Orientation {
    f0: SQRT_3,
    f1: SQRT_3 / 2.0,
    f2: 0.0,
    f3: 3.0 / 2.0,
    b0: SQRT_3 / 3.0,
    b1: -1.0 / 3.0,
    b2: 0.0,
    b3: 2.0 / 3.0,
    start_angle: 0.5,
};
pub const layout_flat: Orientation = Orientation {
    f0: 3.0 / 2.0,
    f1: 0.0,
    f2: SQRT_3 / 2.0,
    f3: SQRT_3,
    b0: 2.0 / 3.0,
    b1: 0.0,
    b2: -1.0 / 3.0,
    b3: SQRT_3 / 3.0,
    start_angle: 0.0,
};
pub fn hex_to_pixel(layout: Layout, h: Hex) -> Point {
    let mut M: Orientation = layout.orientation;
    let mut size: Point = layout.size;
    let mut origin: Point = layout.origin;
    let mut x: f64 = (M.f0 * h.q as f64 + M.f1 * h.r as f64) * size.x;
    let mut y: f64 = (M.f2 * h.q as f64 + M.f3 * h.r as f64) * size.y;
    Point {
        x: x + origin.x,
        y: y + origin.y,
    }
}

pub fn pixel_to_hex(layout: Layout, p: Point) -> FractionalHex {
    let mut M: Orientation = layout.orientation;
    let mut size: Point = layout.size;
    let mut origin: Point = layout.origin;
    let mut pt: Point = Point {
        x: (p.x - origin.x) / size.x,
        y: (p.y - origin.y) / size.y,
    };
    let mut q: f64 = M.b0 * pt.x + M.b1 * pt.y;
    let mut r: f64 = M.b2 * pt.x + M.b3 * pt.y;
    FractionalHex { q, r, s: -q - r }
}

pub fn hex_corner_offset(layout: Layout, corner: i32) -> Point {
    let mut M: Orientation = layout.orientation;
    let mut size: Point = layout.size;
    let mut angle: f64 = 2.0 * PI * (M.start_angle - corner as f64) / 6.0;
    Point {
        x: size.x * angle.cos(),
        y: size.y * angle.sin(),
    }
}

pub fn polygon_corners(layout: Layout, h: Hex) -> Vec<Point> {
    let mut corners: Vec<Point> = vec![];
    let mut center: Point = hex_to_pixel(layout, h);
    for i in 0..6 {
        let mut offset: Point = hex_corner_offset(layout, i);
        corners.push(Point {
            x: center.x + offset.x,
            y: center.y + offset.y,
        });
    }
    corners
}

// Tests
#[cfg(test)]
mod tests {

    use super::*;

    fn complain(name: &str) {
        println!("FAIL {}", name);
    }

    pub fn equal_hex(name: &str, a: Hex, b: Hex) {
        if !(a.q == b.q && a.s == b.s && a.r == b.r) {
            complain(name);
        }
    }

    pub fn equal_offsetcoord(name: &str, a: OffsetCoord, b: OffsetCoord) {
        if !(a.col == b.col && a.row == b.row) {
            complain(name);
        }
    }

    pub fn equal_doubledcoord(name: &str, a: DoubledCoord, b: DoubledCoord) {
        if !(a.col == b.col && a.row == b.row) {
            complain(name);
        }
    }

    pub fn equal_int(name: &str, a: i32, b: i32) {
        if a != b {
            complain(name);
        }
    }

    pub fn equal_hex_array(name: &str, a: Vec<Hex>, b: Vec<Hex>) {
        equal_int(name, a.len() as i32, b.len() as i32);
        for i in 0..(a.len() as i32) {
            equal_hex(name, a[i as usize], b[i as usize]);
        }
    }

    #[test]
    pub fn test_hex_arithmetic() {
        equal_hex(
            "hex_add",
            Hex { q: 4, r: -10, s: 6 },
            hex_add(Hex { q: 1, r: -3, s: 2 }, Hex { q: 3, r: -7, s: 4 }),
        );
        equal_hex(
            "hex_subtract",
            Hex { q: -2, r: 4, s: -2 },
            hex_subtract(Hex { q: 1, r: -3, s: 2 }, Hex { q: 3, r: -7, s: 4 }),
        );
    }

    #[test]
    pub fn test_hex_direction() {
        equal_hex("hex_direction", Hex { q: 0, r: -1, s: 1 }, hex_direction(2));
    }

    #[test]
    pub fn test_hex_neighbor() {
        equal_hex(
            "hex_neighbor",
            Hex { q: 1, r: -3, s: 2 },
            hex_neighbor(Hex { q: 1, r: -2, s: 1 }, 2),
        );
    }

    #[test]
    pub fn test_hex_diagonal() {
        equal_hex(
            "hex_diagonal",
            Hex { q: -1, r: -1, s: 2 },
            hex_diagonal_neighbor(Hex { q: 1, r: -2, s: 1 }, 3),
        );
    }

    #[test]
    pub fn test_hex_distance() {
        equal_int(
            "hex_distance",
            7,
            hex_distance(Hex { q: 3, r: -7, s: 4 }, Hex { q: 0, r: 0, s: 0 }),
        );
    }

    #[test]
    pub fn test_hex_rotate_right() {
        equal_hex(
            "hex_rotate_right",
            hex_rotate_right(Hex { q: 1, r: -3, s: 2 }),
            Hex { q: 3, r: -2, s: -1 },
        );
    }

    #[test]
    pub fn test_hex_rotate_left() {
        equal_hex(
            "hex_rotate_left",
            hex_rotate_left(Hex { q: 1, r: -3, s: 2 }),
            Hex { q: -2, r: -1, s: 3 },
        );
    }

    #[test]
    pub fn test_hex_round() {
        let mut a: FractionalHex = FractionalHex {
            q: 0.0,
            r: 0.0,
            s: 0.0,
        };
        let mut b: FractionalHex = FractionalHex {
            q: 1.0,
            r: -1.0,
            s: 0.0,
        };
        let mut c: FractionalHex = FractionalHex {
            q: 0.0,
            r: -1.0,
            s: 1.0,
        };
        equal_hex(
            "hex_round 1",
            Hex { q: 5, r: -10, s: 5 },
            hex_round(hex_lerp(
                FractionalHex {
                    q: 0.0,
                    r: 0.0,
                    s: 0.0,
                },
                FractionalHex {
                    q: 10.0,
                    r: -20.0,
                    s: 10.0,
                },
                0.5,
            )),
        );
        equal_hex(
            "hex_round 2",
            hex_round(a),
            hex_round(hex_lerp(a, b, 0.499)),
        );
        equal_hex(
            "hex_round 3",
            hex_round(b),
            hex_round(hex_lerp(a, b, 0.501)),
        );
        equal_hex(
            "hex_round 4",
            hex_round(a),
            hex_round(FractionalHex {
                q: a.q * 0.4 + b.q * 0.3 + c.q * 0.3,
                r: a.r * 0.4 + b.r * 0.3 + c.r * 0.3,
                s: a.s * 0.4 + b.s * 0.3 + c.s * 0.3,
            }),
        );
        equal_hex(
            "hex_round 5",
            hex_round(c),
            hex_round(FractionalHex {
                q: a.q * 0.3 + b.q * 0.3 + c.q * 0.4,
                r: a.r * 0.3 + b.r * 0.3 + c.r * 0.4,
                s: a.s * 0.3 + b.s * 0.3 + c.s * 0.4,
            }),
        );
    }

    #[test]
    pub fn test_hex_linedraw() {
        equal_hex_array(
            "hex_linedraw",
            vec![
                Hex { q: 0, r: 0, s: 0 },
                Hex { q: 0, r: -1, s: 1 },
                Hex { q: 0, r: -2, s: 2 },
                Hex { q: 1, r: -3, s: 2 },
                Hex { q: 1, r: -4, s: 3 },
                Hex { q: 1, r: -5, s: 4 },
            ],
            hex_linedraw(Hex { q: 0, r: 0, s: 0 }, Hex { q: 1, r: -5, s: 4 }),
        );
    }

    #[test]
    pub fn test_layout() {
        let mut h: Hex = Hex { q: 3, r: 4, s: -7 };
        let mut flat: Layout = Layout {
            orientation: layout_flat,
            size: Point { x: 10.0, y: 15.0 },
            origin: Point { x: 35.0, y: 71.0 },
        };
        equal_hex(
            "layout",
            h,
            hex_round(pixel_to_hex(flat, hex_to_pixel(flat, h))),
        );
        let mut pointy: Layout = Layout {
            orientation: layout_pointy,
            size: Point { x: 10.0, y: 15.0 },
            origin: Point { x: 35.0, y: 71.0 },
        };
        equal_hex(
            "layout",
            h,
            hex_round(pixel_to_hex(pointy, hex_to_pixel(pointy, h))),
        );
    }

    #[test]
    pub fn test_offset_roundtrip() {
        let mut a: Hex = Hex { q: 3, r: 4, s: -7 };
        let mut b: OffsetCoord = OffsetCoord { col: 1, row: -3 };
        equal_hex(
            "conversion_roundtrip even-q",
            a,
            qoffset_to_cube(EVEN, qoffset_from_cube(EVEN, a)),
        );
        equal_offsetcoord(
            "conversion_roundtrip even-q",
            b,
            qoffset_from_cube(EVEN, qoffset_to_cube(EVEN, b)),
        );
        equal_hex(
            "conversion_roundtrip odd-q",
            a,
            qoffset_to_cube(ODD, qoffset_from_cube(ODD, a)),
        );
        equal_offsetcoord(
            "conversion_roundtrip odd-q",
            b,
            qoffset_from_cube(ODD, qoffset_to_cube(ODD, b)),
        );
        equal_hex(
            "conversion_roundtrip even-r",
            a,
            roffset_to_cube(EVEN, roffset_from_cube(EVEN, a)),
        );
        equal_offsetcoord(
            "conversion_roundtrip even-r",
            b,
            roffset_from_cube(EVEN, roffset_to_cube(EVEN, b)),
        );
        equal_hex(
            "conversion_roundtrip odd-r",
            a,
            roffset_to_cube(ODD, roffset_from_cube(ODD, a)),
        );
        equal_offsetcoord(
            "conversion_roundtrip odd-r",
            b,
            roffset_from_cube(ODD, roffset_to_cube(ODD, b)),
        );
    }

    #[test]
    pub fn test_offset_from_cube() {
        equal_offsetcoord(
            "offset_from_cube even-q",
            OffsetCoord { col: 1, row: 3 },
            qoffset_from_cube(EVEN, Hex { q: 1, r: 2, s: -3 }),
        );
        equal_offsetcoord(
            "offset_from_cube odd-q",
            OffsetCoord { col: 1, row: 2 },
            qoffset_from_cube(ODD, Hex { q: 1, r: 2, s: -3 }),
        );
    }

    #[test]
    pub fn test_offset_to_cube() {
        equal_hex(
            "offset_to_cube even-",
            Hex { q: 1, r: 2, s: -3 },
            qoffset_to_cube(EVEN, OffsetCoord { col: 1, row: 3 }),
        );
        equal_hex(
            "offset_to_cube odd-q",
            Hex { q: 1, r: 2, s: -3 },
            qoffset_to_cube(ODD, OffsetCoord { col: 1, row: 2 }),
        );
    }

    #[test]
    pub fn test_doubled_roundtrip() {
        let mut a: Hex = Hex { q: 3, r: 4, s: -7 };
        let mut b: DoubledCoord = DoubledCoord { col: 1, row: -3 };
        equal_hex(
            "conversion_roundtrip doubled-q",
            a,
            qdoubled_to_cube(qdoubled_from_cube(a)),
        );
        equal_doubledcoord(
            "conversion_roundtrip doubled-q",
            b,
            qdoubled_from_cube(qdoubled_to_cube(b)),
        );
        equal_hex(
            "conversion_roundtrip doubled-r",
            a,
            rdoubled_to_cube(rdoubled_from_cube(a)),
        );
        equal_doubledcoord(
            "conversion_roundtrip doubled-r",
            b,
            rdoubled_from_cube(rdoubled_to_cube(b)),
        );
    }

    #[test]
    pub fn test_doubled_from_cube() {
        equal_doubledcoord(
            "doubled_from_cube doubled-q",
            DoubledCoord { col: 1, row: 5 },
            qdoubled_from_cube(Hex { q: 1, r: 2, s: -3 }),
        );
        equal_doubledcoord(
            "doubled_from_cube doubled-r",
            DoubledCoord { col: 4, row: 2 },
            rdoubled_from_cube(Hex { q: 1, r: 2, s: -3 }),
        );
    }

    #[test]
    pub fn test_doubled_to_cube() {
        equal_hex(
            "doubled_to_cube doubled-q",
            Hex { q: 1, r: 2, s: -3 },
            qdoubled_to_cube(DoubledCoord { col: 1, row: 5 }),
        );
        equal_hex(
            "doubled_to_cube doubled-r",
            Hex { q: 1, r: 2, s: -3 },
            rdoubled_to_cube(DoubledCoord { col: 4, row: 2 }),
        );
    }
}
