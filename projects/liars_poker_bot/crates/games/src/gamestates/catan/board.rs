//! Board geometry and the fixed "beginner" layout for Settlers of Catan.
//!
//! The standard board is a hexagon of 19 hexes (rows of 3,4,5,4,3) with 54
//! vertices and 72 edges. Hexes use axial coordinates (q, r) with r in -2..=2.
//! Every vertex is canonically the North or South corner of exactly one hex
//! coordinate (possibly off-board), which gives a unique (q, r, side) name for
//! each vertex. All adjacency tables are computed once and cached.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

pub const NUM_HEXES: usize = 19;
pub const NUM_VERTICES: usize = 54;
pub const NUM_EDGES: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Resource {
    Brick = 0,
    Lumber = 1,
    Ore = 2,
    Grain = 3,
    Wool = 4,
}

pub const RESOURCES: [Resource; 5] = [
    Resource::Brick,
    Resource::Lumber,
    Resource::Ore,
    Resource::Grain,
    Resource::Wool,
];

impl Resource {
    pub fn from_index(i: usize) -> Resource {
        RESOURCES[i]
    }

    pub fn char(&self) -> char {
        match self {
            Resource::Brick => 'B',
            Resource::Lumber => 'L',
            Resource::Ore => 'O',
            Resource::Grain => 'G',
            Resource::Wool => 'W',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Port {
    ThreeToOne,
    TwoToOne(Resource),
}

/// Terrain of a hex: either produces a resource or is the desert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Terrain {
    Producing(Resource),
    Desert,
}

/// Vertex side: every vertex is the N or S corner of its canonical hex coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Side {
    North,
    South,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct VCoord {
    // Sort by row first so vertex indices increase top-to-bottom.
    r: i8,
    s: Side,
    q: i8,
}

fn hex_on_board(q: i8, r: i8) -> bool {
    q.abs() <= 2 && r.abs() <= 2 && (q + r).abs() <= 2
}

/// The three hex coordinates that meet at a vertex (some may be off-board).
fn vertex_hex_coords(v: VCoord) -> [(i8, i8); 3] {
    match v.s {
        Side::North => [(v.q, v.r), (v.q, v.r - 1), (v.q + 1, v.r - 1)],
        Side::South => [(v.q, v.r), (v.q, v.r + 1), (v.q - 1, v.r + 1)],
    }
}

/// The three vertices adjacent to a vertex (some may be off-board).
fn vertex_neighbor_coords(v: VCoord) -> [VCoord; 3] {
    let (q, r) = (v.q, v.r);
    match v.s {
        Side::North => [
            VCoord { q, r: r - 1, s: Side::South },
            VCoord { q: q + 1, r: r - 1, s: Side::South },
            VCoord { q: q + 1, r: r - 2, s: Side::South },
        ],
        Side::South => [
            VCoord { q, r: r + 1, s: Side::North },
            VCoord { q: q - 1, r: r + 1, s: Side::North },
            VCoord { q: q - 1, r: r + 2, s: Side::North },
        ],
    }
}

/// Corners of hex (q, r) in clockwise order: N, NE, SE, S, SW, NW.
fn hex_corner_coords(q: i8, r: i8) -> [VCoord; 6] {
    [
        VCoord { q, r, s: Side::North },
        VCoord { q: q + 1, r: r - 1, s: Side::South },
        VCoord { q, r: r + 1, s: Side::North },
        VCoord { q, r, s: Side::South },
        VCoord { q: q - 1, r: r + 1, s: Side::North },
        VCoord { q, r: r - 1, s: Side::South },
    ]
}

pub struct BoardGeometry {
    /// Axial coordinates of each hex, row-major top-to-bottom.
    pub hex_coords: [(i8, i8); NUM_HEXES],
    /// The 6 vertex indices of each hex (order: N, NE, SE, S, SW, NW).
    pub hex_vertices: [[u8; 6]; NUM_HEXES],
    /// Hexes touching each vertex (1-3 entries).
    pub vertex_hexes: Vec<Vec<u8>>,
    /// Vertices adjacent to each vertex (2-3 entries).
    pub vertex_neighbors: Vec<Vec<u8>>,
    /// Edges incident to each vertex (2-3 entries).
    pub vertex_edges: Vec<Vec<u8>>,
    /// Endpoint vertices of each edge, (low, high).
    pub edge_vertices: [(u8, u8); NUM_EDGES],
    /// Port available at each vertex, if any.
    pub vertex_port: [Option<Port>; NUM_VERTICES],
    /// Terrain of each hex in the beginner layout.
    pub hex_terrain: [Terrain; NUM_HEXES],
    /// Dice number of each hex (0 for the desert).
    pub hex_number: [u8; NUM_HEXES],
    /// Hex index of the desert (robber start).
    pub desert_hex: u8,
}

/// The fixed beginner layout from the base-game rulebook, row-major.
/// (terrain, dice number)
const BEGINNER_LAYOUT: [(Terrain, u8); NUM_HEXES] = [
    // row 0
    (Terrain::Producing(Resource::Ore), 10),
    (Terrain::Producing(Resource::Wool), 2),
    (Terrain::Producing(Resource::Lumber), 9),
    // row 1
    (Terrain::Producing(Resource::Grain), 12),
    (Terrain::Producing(Resource::Brick), 6),
    (Terrain::Producing(Resource::Wool), 4),
    (Terrain::Producing(Resource::Brick), 10),
    // row 2
    (Terrain::Producing(Resource::Grain), 9),
    (Terrain::Producing(Resource::Lumber), 11),
    (Terrain::Desert, 0),
    (Terrain::Producing(Resource::Lumber), 3),
    (Terrain::Producing(Resource::Ore), 8),
    // row 3
    (Terrain::Producing(Resource::Lumber), 8),
    (Terrain::Producing(Resource::Ore), 3),
    (Terrain::Producing(Resource::Grain), 4),
    (Terrain::Producing(Resource::Wool), 5),
    // row 4
    (Terrain::Producing(Resource::Brick), 5),
    (Terrain::Producing(Resource::Grain), 6),
    (Terrain::Producing(Resource::Wool), 11),
];

/// Ports as (hex index, corner a, corner b, port). Corner indices follow the
/// N, NE, SE, S, SW, NW order of `hex_corner_coords`. Positions approximate
/// the rulebook beginner layout: 4 generic 3:1 ports and one 2:1 port per
/// resource, spread around the coast.
const PORTS: [(usize, usize, usize, Port); 9] = [
    (0, 5, 0, Port::ThreeToOne),                    // top-left coast
    (1, 0, 1, Port::TwoToOne(Resource::Grain)),     // top coast
    (6, 0, 1, Port::TwoToOne(Resource::Ore)),       // top-right coast
    (11, 1, 2, Port::ThreeToOne),                   // right coast
    (15, 2, 3, Port::TwoToOne(Resource::Wool)),     // bottom-right coast
    (17, 2, 3, Port::ThreeToOne),                   // bottom coast
    (16, 3, 4, Port::TwoToOne(Resource::Brick)),    // bottom-left coast
    (12, 4, 5, Port::TwoToOne(Resource::Lumber)),   // left coast
    (7, 5, 0, Port::ThreeToOne),                    // upper-left coast
];

fn build_geometry() -> BoardGeometry {
    // Enumerate hexes row-major: r from -2..=2, q ascending within a row.
    let mut hex_coords = [(0i8, 0i8); NUM_HEXES];
    let mut n = 0;
    for r in -2i8..=2 {
        for q in -2i8..=2 {
            if hex_on_board(q, r) {
                hex_coords[n] = (q, r);
                n += 1;
            }
        }
    }
    assert_eq!(n, NUM_HEXES);

    // Collect and index all vertices that touch at least one on-board hex.
    let mut vcoords: Vec<VCoord> = Vec::new();
    for &(q, r) in &hex_coords {
        for c in hex_corner_coords(q, r) {
            if !vcoords.contains(&c) {
                vcoords.push(c);
            }
        }
    }
    vcoords.sort();
    assert_eq!(vcoords.len(), NUM_VERTICES);
    let vid = |c: VCoord| -> u8 { vcoords.iter().position(|&x| x == c).unwrap() as u8 };

    let mut hex_vertices = [[0u8; 6]; NUM_HEXES];
    for (h, &(q, r)) in hex_coords.iter().enumerate() {
        for (i, c) in hex_corner_coords(q, r).into_iter().enumerate() {
            hex_vertices[h][i] = vid(c);
        }
    }

    let mut vertex_hexes = vec![Vec::new(); NUM_VERTICES];
    let mut vertex_neighbors = vec![Vec::new(); NUM_VERTICES];
    for (v, &c) in vcoords.iter().enumerate() {
        for (hq, hr) in vertex_hex_coords(c) {
            if hex_on_board(hq, hr) {
                let h = hex_coords.iter().position(|&x| x == (hq, hr)).unwrap() as u8;
                vertex_hexes[v].push(h);
            }
        }
        for nc in vertex_neighbor_coords(c) {
            if vcoords.contains(&nc) {
                vertex_neighbors[v].push(vid(nc));
            }
        }
        vertex_neighbors[v].sort();
    }

    // Edges: unordered pairs of adjacent vertices.
    let mut edges: Vec<(u8, u8)> = Vec::new();
    for (v, neighbors) in vertex_neighbors.iter().enumerate() {
        for &w in neighbors {
            let e = ((v as u8).min(w), (v as u8).max(w));
            if !edges.contains(&e) {
                edges.push(e);
            }
        }
    }
    edges.sort();
    assert_eq!(edges.len(), NUM_EDGES);
    let mut edge_vertices = [(0u8, 0u8); NUM_EDGES];
    edge_vertices.copy_from_slice(&edges);

    let mut vertex_edges = vec![Vec::new(); NUM_VERTICES];
    for (e, &(a, b)) in edges.iter().enumerate() {
        vertex_edges[a as usize].push(e as u8);
        vertex_edges[b as usize].push(e as u8);
    }

    let mut vertex_port = [None; NUM_VERTICES];
    for (h, ca, cb, port) in PORTS {
        for corner in [ca, cb] {
            let v = hex_vertices[h][corner] as usize;
            assert!(vertex_port[v].is_none(), "two ports on one vertex");
            vertex_port[v] = Some(port);
        }
    }

    let mut hex_terrain = [Terrain::Desert; NUM_HEXES];
    let mut hex_number = [0u8; NUM_HEXES];
    let mut desert_hex = 0;
    for (h, (t, num)) in BEGINNER_LAYOUT.into_iter().enumerate() {
        hex_terrain[h] = t;
        hex_number[h] = num;
        if t == Terrain::Desert {
            desert_hex = h as u8;
        }
    }

    BoardGeometry {
        hex_coords,
        hex_vertices,
        vertex_hexes,
        vertex_neighbors,
        vertex_edges,
        edge_vertices,
        vertex_port,
        hex_terrain,
        hex_number,
        desert_hex,
    }
}

pub fn geometry() -> &'static BoardGeometry {
    static GEOMETRY: OnceLock<BoardGeometry> = OnceLock::new();
    GEOMETRY.get_or_init(build_geometry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_counts() {
        let g = geometry();
        for v in 0..NUM_VERTICES {
            let degree = g.vertex_edges[v].len();
            assert!((2..=3).contains(&degree), "vertex {v} has degree {degree}");
            assert_eq!(g.vertex_neighbors[v].len(), degree);
            let hexes = g.vertex_hexes[v].len();
            assert!((1..=3).contains(&hexes));
        }
        // Each hex has 6 distinct vertices.
        for h in 0..NUM_HEXES {
            let mut vs = g.hex_vertices[h].to_vec();
            vs.dedup();
            assert_eq!(vs.len(), 6);
        }
        // 6 * 19 hex-vertex incidences = sum over vertices of touching hexes.
        let total: usize = g.vertex_hexes.iter().map(|v| v.len()).sum();
        assert_eq!(total, 6 * NUM_HEXES);
        // 18 port vertices: 9 ports * 2 vertices, all distinct (asserted in build).
        let ports = g.vertex_port.iter().filter(|p| p.is_some()).count();
        assert_eq!(ports, 18);
        assert_eq!(g.desert_hex, 9);
    }

    #[test]
    fn beginner_layout_is_standard() {
        let g = geometry();
        // Resource counts: 4 lumber/wool/grain, 3 brick/ore, 1 desert.
        let mut counts = [0; 6];
        for t in g.hex_terrain {
            match t {
                Terrain::Producing(r) => counts[r as usize] += 1,
                Terrain::Desert => counts[5] += 1,
            }
        }
        assert_eq!(counts, [3, 4, 3, 4, 4, 1]); // brick, lumber, ore, grain, wool, desert
        // Number tokens: one 2 and 12, two of each 3-6 and 8-11, no 7.
        let mut nums = [0; 13];
        for n in g.hex_number {
            nums[n as usize] += 1;
        }
        assert_eq!(nums[2], 1);
        assert_eq!(nums[12], 1);
        assert_eq!(nums[7], 0);
        for n in [3, 4, 5, 6, 8, 9, 10, 11] {
            assert_eq!(nums[n], 2, "number {n}");
        }
    }

    #[test]
    fn edges_match_neighbors() {
        let g = geometry();
        for (e, &(a, b)) in g.edge_vertices.iter().enumerate() {
            assert!(a < b);
            assert!(g.vertex_neighbors[a as usize].contains(&b));
            assert!(g.vertex_edges[a as usize].contains(&(e as u8)));
            assert!(g.vertex_edges[b as usize].contains(&(e as u8)));
        }
    }
}
