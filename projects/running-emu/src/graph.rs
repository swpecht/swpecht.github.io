use std::{
    cmp::{max, min},
    collections::HashSet,
    iter::FromIterator,
};

use hecs::World;
use itertools::Itertools;

use crate::{get_max_point, spatial::Point, Attack, AttackerAgent, Health, Position, Visibility};

/// Underlying datastructure used for path finding
#[derive(Debug)]
pub struct CostMap {
    /// Vector of the list of neighbors, indexed as `x + y * height`
    successors: Vec<[Option<(Point, i32, EdgeType)>; 4]>,
    pub width: usize,
    pub height: usize,
}

impl CostMap {
    pub fn from_world(world: &World) -> Self {
        let max_p = get_max_point(world);
        let width = max_p.x;
        let height = max_p.y;

        let mut vis_mask = vec![vec![false; width]; height];
        CostMap::populate_vis_mask(&mut vis_mask, world);

        // Amount of damage taken from standing on a tile for 1 turn
        let mut dmg_mask = vec![vec![0; width]; height];
        // Only iterating over visible units with attacks
        for (_, (pos, _, attack)) in world
            .query::<(&Position, &Visibility, &Attack)>()
            .without::<AttackerAgent>()
            .into_iter()
            .filter(|(_, (_, vis, _))| vis.0)
        {
            for y in max(0, pos.0.y as i32 - attack.range as i32) as usize
                ..min(max_p.y, pos.0.y + attack.range + 1)
            {
                for x in max(0, pos.0.x as i32 - attack.range as i32) as usize
                    ..min(max_p.x, pos.0.x + attack.range + 1)
                {
                    let p = Point { x: x, y: y };

                    if pos.0.dist(&p) > attack.range as i32 {
                        continue; // Out of range
                    }

                    dmg_mask[p.y][p.x] = attack.damage;
                }
            }
        }

        let mut health_mask = vec![vec![0; width]; height];
        // For visible entities
        for (_, (pos, _, health)) in world
            .query::<(&Position, &Visibility, &Health)>()
            .without::<AttackerAgent>()
            .into_iter()
            .filter(|(_, (_, vis, _))| vis.0)
        {
            health_mask[pos.0.y][pos.0.x] = health.0;
        }

        let mut g = CostMap {
            successors: vec![[None; 4]; width * height],
            width: max_p.x,
            height: max_p.y,
        };

        // Apply the costs
        for y in 0..height {
            for x in 0..width {
                let to = Point { x: x, y: y };
                for from in get_neighbors(to, width, height) {
                    // The cost to travel to a node is:
                    // the damage you receive upon arriving + the damage you'll take while killing whatever is on the tile + 1 (for travel)
                    let cost = dmg_mask[to.y][to.x]
                        + health_mask[to.y][to.x] * dmg_mask[from.y][from.x]
                        + 1;
                    let edge_type = match vis_mask[from.y][from.x] && vis_mask[to.y][to.x] {
                        true => EdgeType::Visible,
                        _ => EdgeType::Fog,
                    };
                    g.set_cost(from, to, cost, edge_type);
                }
            }
        }

        return g;
    }

    /// Helper function for testing
    pub fn _from_vec(vec: &Vec<Vec<i32>>) -> Self {
        let width = vec[0].len();
        let height = vec.len();

        let mut g = CostMap {
            successors: vec![[None; 4]; width * height],
            width: width,
            height: height,
        };

        for y in 0..height {
            for x in 0..width {
                let p = Point { x: x, y: y };
                for n in get_neighbors(p, width, height) {
                    // Check if connection
                    let mut cost = vec[y][x];
                    cost = max(cost, 0) + 1; // At least 1 cost to move
                    g.set_cost(n, p, cost, EdgeType::Visible);
                }
            }
        }

        return g;
    }

    fn populate_vis_mask(mask: &mut Vec<Vec<bool>>, world: &World) {
        for (_, (pos, vis)) in world.query::<(&Position, &Visibility)>().into_iter() {
            mask[pos.0.y][pos.0.x] = mask[pos.0.y][pos.0.x] || vis.0
        }
    }

    #[inline]
    fn get_successors_mut(&mut self, p: Point) -> &mut [Option<(Point, i32, EdgeType)>; 4] {
        return &mut self.successors[p.x + p.y * self.width];
    }

    /// Returns a list of successors and the cost to move to each
    fn get_successors(&self, p: Point) -> [Option<(Point, i32, EdgeType)>; 4] {
        return self.successors[p.x + p.y * self.width];
    }

    /// Returns a list of predecessors and the cost to move to each
    fn get_predecessors(&self, p: Point) -> [Option<(Point, i32, EdgeType)>; 4] {
        // Graph is always bi directional, so can use successort to look up predecessors
        let successors = self.get_successors(p);
        let mut predecessors = [None; 4];

        for (s, _, _) in successors.iter().filter_map(|x| x.as_ref()) {
            // Should be fine since bi-directional
            let (cost, t) = self.get_edge(*s, p).unwrap();
            let index = CostMap::get_index(*s, p).unwrap();
            predecessors[index] = Some((*s, cost, t));
        }
        return predecessors;
    }

    /// Get the cost to move from one node to another
    fn get_edge(&self, from: Point, to: Point) -> Option<(i32, EdgeType)> {
        let neighbors = &self.get_successors(from);
        let index = CostMap::get_index(from, to)?;
        let (_, c, t) = neighbors[index]?;
        return Some((c, t));
    }

    /// Sets the transition cost
    fn set_cost(&mut self, from: Point, to: Point, cost: i32, edge_type: EdgeType) {
        let neighbors = self.get_successors_mut(from);
        // Invalid set cost call
        let index = CostMap::get_index(from, to).unwrap();
        neighbors[index] = Some((to, cost, edge_type));
    }

    /// Get the index for where a neighbor is stored
    fn get_index(from: Point, to: Point) -> Option<usize> {
        let x_diff = to.x as i32 - from.x as i32;
        let y_diff = to.y as i32 - from.y as i32;

        return match (x_diff, y_diff) {
            (1, 0) => Some(0),
            (-1, 0) => Some(1),
            (0, -1) => Some(2),
            (0, 1) => Some(3),
            _ => None,
        };
    }
}

/// Return positions that are neighbors to a givne position
pub fn get_neighbors(point: Point, width: usize, height: usize) -> Vec<Point> {
    // Allow adjacent and diagonal movement
    // let directions = [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let directions = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    let mut neighbors: Vec<Point> = Vec::new();

    for d in directions {
        let candidate = Point {
            x: (point.x as i32 + d.0) as usize,
            y: (point.y as i32 + d.1) as usize,
        };
        if candidate.x < width && candidate.y < (height) {
            neighbors.push(candidate);
        }
    }

    return neighbors;
}

/// Options for handling fog of war tiles
#[derive(PartialEq, Eq, Clone, Copy, Debug, Hash)]
pub enum EdgeType {
    /// Both nodes are visible
    Visible,
    /// At least one node is in fog
    Fog,
}

/// Read-only view of a cost map that can filter by edge type
pub struct CostMapView<'a> {
    cost_map: &'a CostMap,
    allowed_edges: HashSet<EdgeType>,
    pub width: usize,
    pub height: usize,
}

impl<'a> CostMapView<'a> {
    pub fn new(cost_map: &'a CostMap, allowed_edges: Vec<EdgeType>) -> CostMapView {
        return Self {
            cost_map,
            allowed_edges: HashSet::from_iter(allowed_edges.iter().cloned()),
            width: cost_map.width,
            height: cost_map.height,
        };
    }

    pub fn get_successors(&self, p: Point) -> Vec<(Point, i32)> {
        let successors = self.cost_map.get_successors(p);
        return self.filter_edges(successors);
    }

    pub fn get_predecessors(&self, p: Point) -> Vec<(Point, i32)> {
        let predecessors = self.cost_map.get_predecessors(p);
        return self.filter_edges(predecessors);
    }

    fn filter_edges(&self, edges: [Option<(Point, i32, EdgeType)>; 4]) -> Vec<(Point, i32)> {
        let results = edges
            .iter()
            .filter_map(|x| x.as_ref())
            .filter(|(_, _, t)| self.allowed_edges.contains(t))
            .map(|(p, c, _)| (*p, *c))
            .collect_vec();
        return results;
    }

    pub fn get_cost(&self, from: Point, to: Point) -> Option<i32> {
        let successors = self.get_successors(from);
        let (_, cost) = successors.iter().find(|(p, _)| *p == to)?;
        return Some(*cost);
    }

    /// Returns a list of nodes where predecessors have changed
    pub fn get_changed_nodes(&self, other: &CostMapView) -> Vec<Point> {
        let mut changed = Vec::new();

        for y in 0..self.height {
            for x in 0..self.width {
                let p = Point { x: x, y: y };
                let o_pred = other.get_predecessors(p);
                let s_pred = self.get_predecessors(p);

                if o_pred != s_pred {
                    changed.push(p)
                }
            }
        }
        return changed;
    }
}
