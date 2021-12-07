use std::{
    cmp::{max, min, Reverse},
    collections::{HashMap, HashSet},
};

use crossterm::style::Color;
use hecs::World;
use itertools::Itertools;
use priority_queue::PriorityQueue;

use crate::{
    get_goal, get_max_point, get_start,
    spatial::{get_entities, Point},
    Attack, AttackerAgent, BackgroundHighlight, Damage, FeatureFlags, Health, PathingAlgorithm,
    Position, TargetLocation, Visibility,
};

/// Move agents that have a target location and attack if needed.
///
/// This system handles the pathfinding part of the AI. It doesn't select where agents should go
/// only how to get there.
pub fn system_ai_action(world: &mut World) {
    let tile_costs = CostMap::from_world(&world, InvisibleTileTreatment::Impassible);
    let mut health_entities = Vec::new();

    for (e, (pos, _)) in world.query_mut::<(&Position, &Health)>() {
        health_entities.push((e, pos.0))
    }

    // Agents that can attack, attack if a health entity in front, otherwise they move
    let mut attacks_to_apply = Vec::new();
    for (e, (pos, target, attack)) in
        world.query_mut::<(&mut Position, &mut TargetLocation, &Attack)>()
    {
        if target.0.is_none() {
            continue;
        }

        let path = get_path(pos.0, target.0.unwrap(), &tile_costs).unwrap();
        let target_move = path[1];
        if let Some((target, _)) = health_entities.iter().find(|(_, p)| *p == target_move) {
            attacks_to_apply.push((
                target,
                Damage {
                    amount: attack.damage,
                    from: e,
                },
            ));
        } else {
            // Nothing in the way, can move
            pos.0 = target_move;
        }

        if target.0.unwrap() == pos.0 {
            target.0 = None // Reach goal
        }
    }

    for (target, dmg) in attacks_to_apply {
        world.insert_one(*target, dmg).unwrap();
    }
}

/// Identify where agents should move next to explore.
///
/// This system sets target locations and handles route highlighting.
///
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
pub fn system_exploration(
    world: &mut World,
    features: FeatureFlags,
    pather_start: &mut LpaStarPather,
    pather_goal: &mut LpaStarPather,
) -> bool {
    let agent_ids = world
        .query_mut::<&AttackerAgent>()
        .into_iter()
        .collect_vec();
    let agent_id = agent_ids[0].0; // Since only 1 agent

    let cur_loc = world.get::<Position>(agent_id).unwrap().0;
    let goal = get_goal(world);
    let start = get_start(world);
    if cur_loc == goal {
        return true; // Found the goal
    }

    let target_loc = world.get::<TargetLocation>(agent_id).unwrap().0;

    // Generate the next target if we're there or don't have a goal.
    //
    // The next target is chosen by constructing a matrix of scores for all possible explored locations and choosing the minimum
    // An explored location will not be chosen. The scores for squares have the following form:
    //  Score(point) = cost to get there from start + distance from the agent + cost to get to goal assuming un-explored squares have only travel cost
    if target_loc.is_none() || cur_loc == target_loc.unwrap() {
        let start_costs = CostMap::from_world(&world, InvisibleTileTreatment::Impassible);
        let candidate_points = get_edge_points(&start_costs, goal);

        let v;
        let start_travel_costs = match features.pathing_algorithm {
            PathingAlgorithm::Astar => {
                v = get_travel_costs(start, &start_costs);
                &v
            }
            PathingAlgorithm::LpaStar => {
                pather_start.update_tile_costs(&start_costs);
                pather_start.get_travel_costs()
            }
        };

        // Assume unseen tiles are empty
        let goal_costs = CostMap::from_world(&world, InvisibleTileTreatment::Empty);

        let v;
        let goal_travel_costs = match features.pathing_algorithm {
            PathingAlgorithm::Astar => {
                v = get_travel_costs(goal, &goal_costs);
                &v
            }
            PathingAlgorithm::LpaStar => {
                pather_goal.update_tile_costs(&goal_costs);
                pather_goal.get_travel_costs()
            }
        };

        let mut candidate_scores = Vec::with_capacity(candidate_points.len());
        for p in candidate_points.iter() {
            let score = CandidateScore {
                dist_to_start: start_travel_costs[p.y][p.x],
                dist_to_goal: goal_travel_costs[p.y][p.x],
                dist_to_agent: p.dist(&cur_loc),
            };
            candidate_scores.push(score);
        }

        let min_val = *candidate_scores.iter().min().unwrap();
        let min_index = candidate_scores.iter().position(|x| *x == min_val).unwrap();
        let min_p = candidate_points[min_index];
        world
            .insert_one(agent_id, TargetLocation(Some(min_p)))
            .unwrap();
    }

    return false;
}

/// Returns visible points with at least one invisible neighbor
///
/// The Goal is treated as a special case since it's always visible. Goal is only returned
/// if there is at least one visible square near it.
fn get_edge_points(tile_costs: &CostMap, goal: Point) -> Vec<Point> {
    let width = tile_costs.width;
    let height = tile_costs.height;

    let mut points = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let p = Point { x: x, y: y };
            let n = tile_costs.get_neighbors(p).len();
            let is_edge = match p {
                p if (p.x == width - 1 || p.x == 0) && (p.y == 0 || p.y == height - 1) => n < 2, // corner point, 2 edges
                p if p.x == 0 || p.y == 0 => n < 3, // side point, 3 edges
                p if p.x == width - 1 || p.y == height - 1 => n < 3, // side point, 3 edges
                p if p == goal => n > 0,            // Goal is valid if at least one connected point
                _ => n < 4,
            };

            if is_edge {
                points.push(p)
            }
        }
    }

    return points;
}

/// Helper for sorting candidate locations.
#[derive(PartialEq, Eq, Clone, Copy)]
struct CandidateScore {
    dist_to_start: i32,
    dist_to_goal: i32,
    dist_to_agent: i32,
}

impl Ord for CandidateScore {
    /// Compare on dist to goal + dist to start, if a tiebreaker, use goal dist as secondary sort, if still tied, used agent dist
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let self_total = self.dist_to_start + self.dist_to_goal;
        let other_total = other
            .dist_to_start
            .checked_add(other.dist_to_goal)
            .unwrap_or(i32::MAX);
        if self_total != other_total {
            return self_total.cmp(&other_total);
        } else if self.dist_to_goal != other.dist_to_goal {
            return self.dist_to_goal.cmp(&other.dist_to_goal);
        } else {
            return self.dist_to_agent.cmp(&other.dist_to_agent);
        }
    }
}

impl PartialOrd for CandidateScore {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Highlight target locations and expected path, useful for debugging
///
/// Only highlights tiles with a sprite
pub fn system_path_highlight(world: &mut World) {
    let mut path_points = Vec::new();
    let mut goal_points = Vec::new();
    let tile_costs = CostMap::from_world(&world, InvisibleTileTreatment::Impassible);

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
        let e = get_entities(world, p);
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
fn get_path(start: Point, end: Point, costs: &CostMap) -> Option<Vec<Point>> {
    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    let width = costs.width;
    let height = costs.height;
    let mut distance_matrix = vec![vec![None; width]; height];
    distance_matrix[start.y][start.x] = Some(0);

    while !queue.is_empty() {
        let (node, _) = queue.pop().unwrap();
        let distance = distance_matrix[node.y][node.x].unwrap();
        if node == end {
            break; // found the goal
        }

        for (n, cost) in costs.get_neighbors(node) {
            let d = distance_matrix[n.y][n.x];

            // Only path over areas where we have cost data
            if d.is_none() {
                let new_cost = distance + cost;
                distance_matrix[n.y][n.x] = Some(new_cost);
                // Distance heuristic for A*
                let goal_dist =
                    (end.x as i32 - n.x as i32).abs() + (end.y as i32 - n.y as i32).abs();
                queue.push(*n, Reverse(goal_dist + new_cost));
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

/// Return the lowest travel cost matrix for all visible tiles if possible
///
/// None means no path is possible or there isn't tile information
fn get_travel_costs(start: Point, tile_costs: &CostMap) -> Vec<Vec<i32>> {
    let width = tile_costs.width;
    let height = tile_costs.height;
    let mut travel_costs = vec![vec![i32::MAX; width]; height];

    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    travel_costs[start.y][start.x] = 0;

    while !queue.is_empty() {
        let (node, _) = queue.pop().unwrap();
        let distance = travel_costs[node.y][node.x];

        for (n, cost) in tile_costs.get_neighbors(node) {
            let d = travel_costs[n.y][n.x];

            let new_cost = distance.checked_add(*cost).unwrap_or(i32::MAX);
            travel_costs[n.y][n.x] = min(d, new_cost);
            if new_cost < d {
                queue.push(*n, Reverse(new_cost));
            }
        }
    }

    return travel_costs;
}

/// Underlying datastructure used for path finding
#[derive(Debug)]
struct CostMap {
    /// Vector of the list of neighbors, indexed as `x + y * height`
    adjacency_list: Vec<Vec<(Point, i32)>>,
    pub width: usize,
    pub height: usize,
}

#[derive(PartialEq, Eq)]
enum InvisibleTileTreatment {
    Impassible,
    Empty,
}

impl CostMap {
    fn from_world(world: &World, tile_treatment: InvisibleTileTreatment) -> Self {
        let max_p = get_max_point(world);
        let width = max_p.x;
        let height = max_p.y;

        let mut g = CostMap {
            adjacency_list: vec![Vec::with_capacity(4); width * height],
            width: max_p.x,
            height: max_p.y,
        };

        let max_p = get_max_point(world);

        let visible_tiles: HashSet<Point> = world
            .query::<(&Position, &Visibility)>()
            .into_iter()
            .filter(|(_, (_, vis))| vis.0)
            .map(|(_, (pos, _))| pos.0)
            .collect();

        // Build the baseline adjacency list, if the node is visible, it can be moved to by neigbors in 1 step
        for (_, (pos, visible)) in world.query::<(&Position, &Visibility)>().into_iter() {
            if visible.0 || tile_treatment == InvisibleTileTreatment::Empty {
                for n in get_neighbors(pos.0, max_p.x, max_p.y) {
                    if !visible_tiles.contains(&n)
                        && tile_treatment == InvisibleTileTreatment::Impassible
                    {
                        continue;
                    }
                    g.set_cost(n, pos.0, 1);
                }
            }
        }

        // Update nodes to include cost of health units
        for (_, (pos, visible, health)) in world
            .query::<(&Position, &Visibility, &Health)>()
            .without::<AttackerAgent>()
            .into_iter()
        {
            // Must be visible to get this info
            if visible.0 {
                for n in get_neighbors(pos.0, max_p.x, max_p.y) {
                    if !visible_tiles.contains(&n)
                        && tile_treatment == InvisibleTileTreatment::Impassible
                    {
                        continue;
                    }
                    g.set_cost(n, pos.0, health.0);
                }
            }
        }

        return g;
    }

    fn _from_vec(vec: &Vec<Vec<i32>>) -> Self {
        let width = vec[0].len();
        let height = vec.len();

        let mut g = CostMap {
            adjacency_list: vec![Vec::with_capacity(4); width * height],
            width: width,
            height: height,
        };

        let width = vec[0].len();
        let height = vec.len();

        for y in 0..height {
            for x in 0..width {
                let p = Point { x: x, y: y };
                for n in get_neighbors(p, width, height) {
                    // Check if connection
                    let mut cost = vec[y][x];
                    cost = max(cost, 0) + 1; // At least 1 cost to move
                    g.set_cost(n, p, cost);
                }
            }
        }

        return g;
    }

    fn get_neighbors_mut(&mut self, p: Point) -> &mut Vec<(Point, i32)> {
        return &mut self.adjacency_list[p.x + p.y * self.width];
    }

    /// Returns a list of neighbors and the cost to move to each
    fn get_neighbors(&self, p: Point) -> &Vec<(Point, i32)> {
        return &self.adjacency_list[p.x + p.y * self.width];
    }

    /// Get the cost to move from one node to another
    fn get_cost(&self, from: Point, to: Point) -> Option<i32> {
        let neighbors = &self.get_neighbors(from);
        let (_, v) = neighbors.iter().find(|(p, _)| *p == to)?;
        return Some(*v);
    }

    /// Sets the transition cost
    fn set_cost(&mut self, from: Point, to: Point, cost: i32) {
        let neighbors = self.get_neighbors_mut(from);
        if let Some(cur_entry) = neighbors.iter().position(|(p, _)| *p == to) {
            neighbors[cur_entry] = (to, cost)
        } else {
            neighbors.push((to, cost))
        }

        assert!(neighbors.len() <= 4); // Shouldn't ever have more than 4 neighbors
    }

    /// Update to match another, and returns the nodes
    /// that have changed, this will add new nodes, it won't remove previous ones
    pub fn update(&mut self, other: &CostMap) -> Vec<Point> {
        let mut changed = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let p = Point { x: x, y: y };
                let neighbors = other.get_neighbors(p);

                let entry = self.get_neighbors_mut(p);
                if entry != neighbors {
                    *entry = neighbors.clone();
                    changed.push(p);
                }
            }
        }

        return changed;
    }
}

/// Prints the minimum cost to get to a given cell
pub fn system_print_tile_costs(world: &World) {
    let costs = CostMap::from_world(world, InvisibleTileTreatment::Impassible);
    for y in 0..costs.height {
        for x in 0..costs.width {
            let p = Point { x: x, y: y };
            let mut cost = i32::MAX;

            let neighbors = costs.get_neighbors(p);
            for (_, c) in neighbors {
                cost = min(cost, *c);
            }

            if cost == i32::MAX {
                cost = -1;
            }
            print! {"{}\t", cost}
        }
        println!("")
    }
}

/// Implementation for LPA*, based on:
/// https://en.wikipedia.org/wiki/Lifelong_Planning_A*
pub struct LpaStarPather {
    queue: PriorityQueue<Point, Reverse<LpaKey>>,
    tile_costs: CostMap,
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
    let g = CostMap::from_world(world, InvisibleTileTreatment::Impassible);

    return LpaStarPather::new(start, goal, g);
}

pub fn get_goal_lpapather(world: &World) -> LpaStarPather {
    let start = get_start(world);
    let goal = get_goal(world);
    let g = CostMap::from_world(world, InvisibleTileTreatment::Empty);

    return LpaStarPather::new(goal, start, g);
}

impl LpaStarPather {
    fn new(start: Point, goal: Point, tile_costs: CostMap) -> Self {
        let width = tile_costs.width;
        let height = tile_costs.height;

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
            for (n, _) in self.tile_costs.get_neighbors(*p) {
                self.rhs[p.y][p.x] = min(
                    self.rhs[p.y][p.x],
                    // Ok to unwrap here, edges are guaranteed to be bi-directional, if not, there is a bug
                    self.get_g(*n)
                        .checked_add(self.tile_costs.get_cost(*n, *p).unwrap())
                        .unwrap_or(i32::MAX),
                )
            }
            self.queue.remove(p);
            if self.get_g(*p) != self.get_rhs(*p) {
                self.queue.push(*p, self.calculate_key(p));
            }
        }
    }

    fn update_tile_costs(&mut self, tile_costs: &CostMap) {
        let changed = self.tile_costs.update(tile_costs);
        for p in changed {
            self.update_node(&p);
        }

        self.compute_shortest_path();
    }

    pub fn get_travel_costs(&self) -> &Vec<Vec<i32>> {
        return &self.g;
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::Point;

    #[test]
    fn test_lpa_no_update() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0; 3]; 3]);
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, tile_costs);

        assert_eq!(pather.g, vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]);
    }

    #[test]
    fn test_lpa_no_update_cost() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 0, 0]]);
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, tile_costs);

        assert_eq!(
            pather.g,
            vec![vec![0, 11, 6], vec![1, 12, 5], vec![2, 3, 4]]
        );
    }

    #[test]
    fn test_lpa_update() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0; 3]; 3]);
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let mut pather = LpaStarPather::new(start, goal, tile_costs);

        assert_eq!(pather.g, vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]);

        // discover a wall
        let updated = CostMap::_from_vec(&vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 10, 0]]);

        pather.update_tile_costs(&updated);

        assert_eq!(
            pather.g,
            vec![vec![0, 11, 12], vec![1, 12, 13], vec![2, 13, 14]]
        );
    }

    #[test]
    fn find_path_no_cost() {
        let g = CostMap::_from_vec(&vec![vec![0; 3]; 3]);

        let path = get_path(Point { x: 0, y: 0 }, Point { x: 2, y: 2 }, &g);
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
        let g = CostMap::_from_vec(&vec![vec![0; 3], vec![10, 10, 0], vec![0; 3]]);

        let path = get_path(Point { x: 0, y: 0 }, Point { x: 2, y: 2 }, &g);
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
        let tile_costs = CostMap::_from_vec(&vec![vec![0; 3]; 3]);
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &tile_costs);
        assert_eq!(
            travel_costs,
            vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]
        )
    }

    #[test]
    fn test_travel_cost_with_tile_costs() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 0, 0]]);
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &tile_costs);
        assert_eq!(
            travel_costs,
            vec![vec![0, 11, 6], vec![1, 12, 5], vec![2, 3, 4]]
        )
    }
}
