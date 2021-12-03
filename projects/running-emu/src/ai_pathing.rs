use std::cmp::{min, Reverse};

use crossterm::style::Color;
use hecs::World;
use itertools::Itertools;
use priority_queue::PriorityQueue;

use crate::{
    get_goal, get_max_point, get_start,
    spatial::{get_entities, Point, SpatialCache},
    Agent, BackgroundHighlight, FeatureFlags, PathingAlgorithm, Position, Sprite, TargetLocation,
    Visibility,
};

/// Move agents that have a target location.
///
/// This system handles the pathfinding part of the AI. It doesn't select where agents should go
/// only how to get there.
pub fn system_pathing(world: &mut World) {
    let tile_costs = get_tile_costs(world);

    for (_, (pos, target)) in world.query_mut::<(&mut Position, &mut TargetLocation)>() {
        if target.0.is_none() || target.0.unwrap() == pos.0 {
            continue;
        }

        let path = get_path(pos.0, target.0.unwrap(), &tile_costs).unwrap();
        pos.0 = path[1];

        if target.0.unwrap() == pos.0 {
            target.0 = None // Reach goal
        }
    }
}

/// Identify where agents should move next to explore.
///
/// This system sets target locations and handles route highlighting.
///
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
pub fn system_ai(
    world: &mut World,
    features: FeatureFlags,
    pather_start: &mut LpaStarPather,
    pather_goal: &mut LpaStarPather,
) -> bool {
    let agent_ids = world.query_mut::<&Agent>().into_iter().collect_vec();
    let agent_id = agent_ids[0].0; // Since only 1 agent

    let cur_loc = world.get::<Position>(agent_id).unwrap().0;
    let goal = get_goal(world);
    let start = get_start(world);
    if cur_loc == goal {
        return true; // Found the goal
    }

    let target_loc = world.get::<TargetLocation>(agent_id).unwrap().0;

    let tile_costs = get_tile_costs(world);

    // Generate the next target if we're there or don't have a goal.
    //
    // The next target is chosen by constructing a matrix of scores for all possible explored locations and choosing the minimum
    // An explored location will not be chosen. The scores for squares have the following form:
    //  Score(point) = cost to get there from start + distance from the agent + cost to get to goal assuming un-explored squares have only travel cost
    if target_loc.is_none() || cur_loc == target_loc.unwrap() {
        let mut candidate_points = Vec::new();
        let max_p = get_max_point(world);
        for y in 0..max_p.y {
            for x in 0..max_p.x {
                let p = Point { x: x, y: y };

                // Don't visit if not visible
                let visible = tile_costs[y][x].is_some();
                if !visible {
                    continue;
                }

                let neighbors = get_neighbors(p, max_p.x, max_p.y);
                let mut all_neighbors_visible = true;
                let mut all_neighbors_invisible = true; // to catch isolated tiles
                for n in neighbors {
                    // tile_costs can act as a mask to determine if the cell is visible or not.
                    let vis = tile_costs[n.y][n.x].is_some();
                    all_neighbors_visible = all_neighbors_visible && vis;
                    all_neighbors_invisible = all_neighbors_invisible && !vis;
                }

                // No reason to visit if all visible and not the goal
                if (all_neighbors_visible && p != goal) || all_neighbors_invisible {
                    continue;
                }

                candidate_points.push(p);
            }
        }

        let mut candidate_matrix = vec![None; max_p.x * max_p.y];
        // Create a cost matrix where unknown tiles have a cost of 1
        let mut goal_tile_costs = vec![vec![0; max_p.x]; max_p.y];
        for y in 0..max_p.y {
            for x in 0..max_p.x {
                goal_tile_costs[y][x] = tile_costs[y][x].unwrap_or(0);
            }
        }

        // Populate invisible tiles as max cost
        let mut start_tile_costs = vec![vec![i32::MAX; max_p.x]; max_p.y];
        for y in 0..max_p.y {
            for x in 0..max_p.x {
                start_tile_costs[y][x] = tile_costs[y][x].unwrap_or(i32::MAX);
            }
        }

        let travel_costs = match features.pathing_algorithm {
            PathingAlgorithm::Astar => get_travel_costs(start, &tile_costs),
            PathingAlgorithm::LpaStar => {
                pather_start.update_tile_costs(&start_tile_costs);
                pather_start.get_travel_costs()
            }
        };
        let goal_travel_costs = match features.pathing_algorithm {
            PathingAlgorithm::Astar => get_travel_costs(
                goal,
                &goal_tile_costs
                    .iter()
                    .map(|x| x.iter().map(|v| Some(*v)).collect_vec())
                    .collect_vec(),
            ),
            PathingAlgorithm::LpaStar => {
                pather_goal.update_tile_costs(&goal_tile_costs);
                pather_goal.get_travel_costs()
            }
        };

        for p in candidate_points {
            let travel_cost = travel_costs[p.y][p.x].unwrap();
            let goal_dist = goal_travel_costs[p.y][p.x].unwrap();
            let agent_dist = p.dist(&cur_loc);
            candidate_matrix[p.x + p.y * max_p.x] = Some(travel_cost + goal_dist + agent_dist);
        }

        let min_val = candidate_matrix
            .iter()
            .filter_map(|c| c.as_ref())
            .min()
            .unwrap();
        let mut min_p = Point { x: 0, y: 0 };
        for y in 0..max_p.y {
            for x in 0..max_p.x {
                if candidate_matrix[x + y * max_p.x] == Some(*min_val) {
                    min_p = Point { x: x, y: y };
                    break;
                }
            }
        }
        world
            .insert_one(agent_id, TargetLocation(Some(min_p)))
            .unwrap();
    }

    return false;
}

/// Highlight target locations and expected path, useful for debugging
///
/// Only highlights tiles with a sprite
pub fn system_path_highlight(world: &mut World, spatial_cache: Option<&SpatialCache>) {
    let mut path_points = Vec::new();
    let mut goal_points = Vec::new();
    let tile_costs = get_tile_costs(world);

    let pathers = world
        .query_mut::<&TargetLocation>()
        .into_iter()
        .map(|(e, loc)| (e, loc.0))
        .collect_vec();

    for pather in pathers {
        let (id, target_loc) = pather;
        if target_loc.is_none() {
            continue;
        }
        let target_point = target_loc.unwrap();

        goal_points.push(target_point);

        let cur_loc = world.get::<Position>(id).unwrap().0;
        let mut path = get_path(cur_loc, target_point, &tile_costs).unwrap();
        path_points.append(&mut path);
    }

    for p in path_points {
        let e = get_entities(world, p, spatial_cache);
        let color = match p {
            p if goal_points.contains(&p) => Color::Green,
            _ => Color::Blue,
        };

        world.insert_one(e[0], BackgroundHighlight(color)).unwrap();
    }
}

/// Find a path between 2 arbitraty points if it exists
///
/// Only navigates through known costs. This is the core pathfinding algorithm
fn get_path(start: Point, end: Point, tile_costs: &Vec<Vec<Option<i32>>>) -> Option<Vec<Point>> {
    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    let width = tile_costs[0].len();
    let height = tile_costs.len();
    let mut distance_matrix = vec![vec![None; width]; height];
    distance_matrix[start.y][start.x] = Some(0);

    while !queue.is_empty() {
        let (node, _) = queue.pop().unwrap();
        let distance = distance_matrix[node.y][node.x].unwrap();
        if node == end {
            break; // found the goal
        }

        let neighbors = get_neighbors(node, width, height);

        for n in neighbors {
            let d = distance_matrix[n.y][n.x];

            // Only path over areas where we have cost data
            if d.is_none() && tile_costs[n.y][n.x].is_some() {
                let new_cost = distance + 1 + tile_costs[n.y][n.x].unwrap(); // Cost always increases by minimum of 1
                distance_matrix[n.y][n.x] = Some(new_cost);
                // Distance heuristic for A*
                let goal_dist =
                    (end.x as i32 - n.x as i32).abs() + (end.y as i32 - n.y as i32).abs();
                queue.push(n, Reverse(goal_dist + new_cost));
            }
        }
    }

    return Some(get_path_from_distances(start, end, &distance_matrix));
}

/// Return positions that are neighbors to a givne position
fn get_neighbors(point: Point, width: usize, height: usize) -> Vec<Point> {
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

/// Debug function to print the travel costs
pub fn print_travel_cost_matrix(world: &World) {
    let start = get_start(world);
    let tile_costs = get_tile_costs(world);
    let travel_costs = get_travel_costs(start, &tile_costs);

    let max_p = get_max_point(world);
    for y in 0..max_p.y {
        for x in 0..max_p.x {
            let p = Point { x: x, y: y };
            print!("{}", travel_costs[p.y][p.x].unwrap_or(-1));
            print!("\t")
        }
        println!("");
    }
}

/// Return optimal path connecting start to end given a set of travel costs.
///
/// Given the cost matrix, we can start at the goal and greedily follow the lowest cost
///  path back to the starting point to get an optimal path.
pub fn get_path_from_distances(
    start: Point,
    end: Point,
    travel_costs: &Vec<Vec<Option<i32>>>,
) -> Vec<Point> {
    let mut path = vec![end];
    let width = travel_costs[0].len();
    let height = travel_costs.len();

    while path.last().unwrap().clone() != start {
        let p = path.last().unwrap();
        let neighbors = get_neighbors(*p, width, height);
        let (_, min) = neighbors
            .iter()
            .filter(|p| !travel_costs[p.y][p.x].is_none())
            .enumerate()
            .min_by(|a, b| travel_costs[a.1.y][a.1.x].cmp(&travel_costs[b.1.y][b.1.x]))
            .unwrap();
        path.push(*min);
    }

    path.reverse();
    return path;
}

/// Helper function to determine the cost of traversing a given unit
fn get_tile_cost(tile: char) -> i32 {
    match tile {
        '.' => 0,
        'S' => 0,
        'G' => 0,
        '@' => 0,
        'W' => 50,
        _ => panic!("Error parsing map, invalid character: {}", &tile),
    }
}

/// Return the lowest travel cost matrix for all visible tiles if possible
///
/// None means no path is possible or there isn't tile information
fn get_travel_costs(start: Point, tile_costs: &Vec<Vec<Option<i32>>>) -> Vec<Vec<Option<i32>>> {
    let width = tile_costs[0].len();
    let height = tile_costs.len();
    let mut travel_costs = vec![vec![None; width]; height];

    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    travel_costs[start.y][start.x] = Some(0);

    while !queue.is_empty() {
        let (node, _) = queue.pop().unwrap();
        let distance = travel_costs[node.y][node.x].unwrap();

        let neighbors = get_neighbors(node, width, height);

        for n in neighbors {
            let d = travel_costs[n.y][n.x];

            // Only path over areas where we have cost data
            if d.is_none() && tile_costs[n.y][n.x].is_some() {
                let new_cost = distance + 1 + tile_costs[n.y][n.x].unwrap(); // Cost always increases by minimum of 1
                travel_costs[n.y][n.x] = Some(new_cost);
                queue.push(n, Reverse(new_cost));
            }
        }
    }

    return travel_costs;
}

/// Return the cost matrix from currently visible tiles
fn get_tile_costs(world: &World) -> Vec<Vec<Option<i32>>> {
    let max_p = get_max_point(world);
    let mut tile_costs = vec![vec![None; max_p.x]; max_p.y];
    for (_, (pos, visible, spr)) in world
        .query::<(&Position, &Visibility, &Sprite)>()
        .into_iter()
    {
        if visible.0 {
            tile_costs[pos.0.y][pos.0.x] = Some(get_tile_cost(spr.0));
        }
    }

    return tile_costs;
}

/// Implementation for LPA*, based on:
/// https://en.wikipedia.org/wiki/Lifelong_Planning_A*
pub struct LpaStarPather {
    queue: PriorityQueue<Point, Reverse<LpaKey>>,
    tile_costs: Vec<Vec<i32>>,
    g: Vec<Vec<i32>>,
    rhs: Vec<Vec<i32>>,
    width: usize,
    height: usize,
    start: Point,
    goal: Point,
}

#[derive(PartialEq, Eq)]
struct LpaKey {
    k1: i32,
    k2: i32,
}

impl Ord for LpaKey {
    /// Returns the larger of k1s, if k1s are equal, then look at k2
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.k1 != other.k1 {
            return self.k1.cmp(&other.k1);
        } else {
            return self.k2.cmp(&other.k2);
        }
    }
}

impl PartialOrd for LpaKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

pub fn get_start_lpapather(world: &World) -> LpaStarPather {
    let start = get_start(world);
    let goal = get_goal(world);
    let tile_costs = get_tile_costs(world);

    let width = tile_costs[0].len();
    let height = tile_costs.len();
    let mut start_tile_costs = vec![vec![i32::MAX; width]; height];
    for y in 0..height {
        for x in 0..width {
            start_tile_costs[y][x] = tile_costs[y][x].unwrap_or(i32::MAX);
        }
    }

    return LpaStarPather::new(start, goal, start_tile_costs);
}

pub fn get_goal_lpapather(world: &World) -> LpaStarPather {
    let start = get_start(world);
    let goal = get_goal(world);
    let tile_costs = get_tile_costs(world);

    let width = tile_costs[0].len();
    let height = tile_costs.len();
    let mut goal_tile_costs = vec![vec![0; width]; height];
    for y in 0..height {
        for x in 0..width {
            goal_tile_costs[y][x] = tile_costs[y][x].unwrap_or(0);
        }
    }

    // Want to calculate distances from the goal, so goal is the start
    return LpaStarPather::new(goal, start, goal_tile_costs);
}

impl LpaStarPather {
    fn new(start: Point, goal: Point, tile_costs: Vec<Vec<i32>>) -> Self {
        let width = tile_costs[0].len();
        let height = tile_costs.len();

        // Ensure using a square map
        for i in 0..height {
            assert_eq!(tile_costs[i].len(), width);
        }

        let queue = PriorityQueue::new();

        // Using i32 max as placeholder for infinity
        let g = vec![vec![i32::MAX; width]; height];
        let rhs = vec![vec![i32::MAX; width]; height];

        let mut pather = Self {
            queue: queue,
            tile_costs: tile_costs,
            g,
            rhs,
            width,
            height,
            start,
            goal,
        };

        pather.rhs[start.y][start.x] = 0;
        pather.queue.push(start, pather.calculate_key(&start));

        pather.compute_shortest_path();

        return pather;
    }

    fn calculate_key(&self, p: &Point) -> Reverse<LpaKey> {
        let k1 = min(self.g[p.y][p.x], self.rhs[p.y][p.x]) + self.goal.dist(p);
        let k2 = min(self.g[p.y][p.x], self.rhs[p.y][p.x]);
        return Reverse(LpaKey { k1: k1, k2: k2 });
    }

    #[inline]
    fn get_rhs(&self, p: Point) -> i32 {
        return self.rhs[p.y][p.x];
    }

    #[inline]
    fn get_g(&self, p: Point) -> i32 {
        return self.g[p.y][p.x];
    }

    fn compute_shortest_path(&mut self) {
        // This needs to be greater than since we're actually comparing the reverse of the keys
        while !self.queue.is_empty()
        // Remove the normal termination logic as we want to populate the entire matrix. May need to revisit for
        // performance reasons.
        // && *self.queue.peek().unwrap().1 > self.calculate_key(&self.goal))
        // || (self.get_rhs(self.goal) != self.get_g(self.goal))
        {
            let node = self.queue.pop().unwrap().0;
            if self.get_g(node) > self.get_rhs(node) {
                self.g[node.y][node.x] = self.get_rhs(node);
                for s in get_neighbors(node, self.width, self.height) {
                    self.update_node(&s);
                }
            } else {
                self.g[node.y][node.x] = i32::MAX;
                self.update_node(&node);
                for s in get_neighbors(node, self.width, self.height) {
                    self.update_node(&s);
                }
            }
        }
    }

    /// Recalculates rhs for a node and removes it from the queue.
    /// If the node has become locally inconsistent, it is (re-)inserted into the queue with its new key.
    fn update_node(&mut self, p: &Point) {
        if *p != self.start {
            self.rhs[p.y][p.x] = i32::MAX;
            for n in get_neighbors(*p, self.width, self.height) {
                self.rhs[p.y][p.x] = min(
                    self.rhs[p.y][p.x],
                    self.get_g(n)
                        .checked_add(self.tile_costs[p.y][p.x].checked_add(1).unwrap_or(i32::MAX))
                        .unwrap_or(i32::MAX),
                )
            }
            self.queue.remove(p);
            if self.get_g(*p) != self.get_rhs(*p) {
                self.queue.push(*p, self.calculate_key(p));
            }
        }
    }

    pub fn update_tile_costs(&mut self, tile_costs: &Vec<Vec<i32>>) {
        for y in 0..self.height {
            for x in 0..self.width {
                if self.tile_costs[y][x] != tile_costs[y][x] {
                    self.tile_costs[y][x] = tile_costs[y][x];
                    self.update_node(&Point { x: x, y: y })
                }
            }
        }

        self.compute_shortest_path();
    }

    pub fn get_travel_costs(&self) -> Vec<Vec<Option<i32>>> {
        let mut travel_costs = vec![vec![None; self.width]; self.height];

        for y in 0..self.height {
            for x in 0..self.width {
                if self.tile_costs[y][x] < i32::MAX {
                    travel_costs[y][x] = Some(self.g[y][x]);
                }
            }
        }

        // Properly mask the goal
        let mut all_neighbors_invisible = true;
        for n in get_neighbors(self.goal, self.width, self.height) {
            all_neighbors_invisible =
                all_neighbors_invisible && self.tile_costs[n.y][n.x] == i32::MAX;
        }

        if all_neighbors_invisible {
            travel_costs[self.goal.y][self.goal.x] = None;
        }

        return travel_costs;
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::Point;

    #[test]
    fn test_lpa_no_update() {
        let tile_costs = vec![vec![0; 3]; 3];
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, tile_costs);

        assert_eq!(pather.g, vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]);
    }

    #[test]
    fn test_lpa_no_update_cost() {
        let tile_costs = vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 0, 0]];
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, tile_costs);

        assert_eq!(
            pather.g,
            vec![vec![0, 11, 6], vec![1, 12, 5], vec![2, 3, 4]]
        );
    }

    #[test]
    fn test_lpa_no_update_hidden() {
        let tile_costs = vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, i32::MAX, 0]];
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, tile_costs);

        assert_eq!(
            pather.get_travel_costs(),
            vec![
                vec![Some(0), Some(11), Some(12)],
                vec![Some(1), Some(12), Some(13)],
                vec![Some(2), None, Some(14)]
            ]
        );
    }

    #[test]
    fn test_lpa_update() {
        let mut tile_costs = vec![vec![0; 3]; 3];
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let mut pather = LpaStarPather::new(start, goal, tile_costs.clone());

        assert_eq!(pather.g, vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]);

        // discover a wall
        tile_costs[0][1] = 10;
        tile_costs[1][1] = 10;
        tile_costs[2][1] = 10;
        pather.update_tile_costs(&tile_costs);

        assert_eq!(
            pather.g,
            vec![vec![0, 11, 12], vec![1, 12, 13], vec![2, 13, 14]]
        );
    }

    #[test]
    fn find_path_no_cost() {
        let path = get_path(
            Point { x: 0, y: 0 },
            Point { x: 2, y: 2 },
            &vec![vec![Some(0); 3]; 3],
        );
        assert_eq!(
            path.unwrap(),
            vec![
                Point { x: 0, y: 0 },
                Point { x: 0, y: 1 },
                Point { x: 0, y: 2 },
                Point { x: 1, y: 2 },
                Point { x: 2, y: 2 }
            ]
        )
    }

    #[test]
    fn find_path_cost() {
        let path = get_path(
            Point { x: 0, y: 0 },
            Point { x: 2, y: 2 },
            &vec![
                vec![Some(0); 3],
                vec![Some(10), Some(10), Some(0)],
                vec![Some(0); 3],
            ],
        );
        assert_eq!(
            path.unwrap(),
            vec![
                Point { x: 0, y: 0 },
                Point { x: 1, y: 0 },
                Point { x: 2, y: 0 },
                Point { x: 2, y: 1 },
                Point { x: 2, y: 2 }
            ]
        )
    }

    #[test]
    fn test_travel_cost_simple() {
        let tile_costs = vec![vec![Some(0); 3]; 3];
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &tile_costs);
        assert_eq!(
            travel_costs,
            vec![
                vec![Some(0), Some(1), Some(2)],
                vec![Some(1), Some(2), Some(3)],
                vec![Some(2), Some(3), Some(4)]
            ]
        )
    }

    #[test]
    fn test_travel_cost_unexplored() {
        let tile_costs = vec![
            vec![Some(0), Some(0), Some(0)],
            vec![Some(0), None, Some(0)],
            vec![Some(0), Some(0), None],
        ];
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &tile_costs);
        assert_eq!(
            travel_costs,
            vec![
                vec![Some(0), Some(1), Some(2)],
                vec![Some(1), None, Some(3)],
                vec![Some(2), Some(3), None]
            ]
        )
    }

    #[test]
    fn test_travel_cost_with_tile_costs() {
        let tile_costs = vec![
            vec![Some(0), Some(10), Some(0)],
            vec![Some(0), Some(10), Some(0)],
            vec![Some(0), Some(0), Some(0)],
        ];
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &tile_costs);
        assert_eq!(
            travel_costs,
            vec![
                vec![Some(0), Some(11), Some(6)],
                vec![Some(1), Some(12), Some(5)],
                vec![Some(2), Some(3), Some(4)]
            ]
        )
    }
}
