use std::cmp::{min, Reverse};

use crossterm::style::Color;
use hecs::World;
use itertools::Itertools;
use priority_queue::PriorityQueue;

use crate::{
    get_goal, get_start,
    graph::{get_neighbors, CostMap, CostMapView, EdgeType},
    spatial::{get_entities, Point},
    Attack, AttackerAgent, BackgroundHighlight, Damage, FeatureFlags, Health, PathingAlgorithm,
    Position, TargetLocation,
};

/// Move agents that have a target location and attack if needed.
///
/// This system handles the pathfinding part of the AI. It doesn't select where agents should go
/// only how to get there.
pub fn system_ai_action(world: &mut World) {
    let tile_costs = CostMap::from_world(&world);
    let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);
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

        let path = get_path(pos.0, target.0.unwrap(), &cost_view).unwrap();
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
        let costs = CostMap::from_world(&world);
        let start_view = CostMapView::new(&costs, vec![EdgeType::Visible]);
        let goal_view = CostMapView::new(&costs, vec![EdgeType::Visible, EdgeType::Fog]);

        let candidate_points = get_edge_points(&start_view, goal);

        let v;
        let start_travel_costs = match features.pathing_algorithm {
            PathingAlgorithm::Astar => {
                v = get_travel_costs(start, &start_view);
                &v
            }
            PathingAlgorithm::LpaStar => {
                pather_start.update_tile_costs(&start_view);
                pather_start.get_travel_costs()
            }
        };

        let v;
        let goal_travel_costs = match features.pathing_algorithm {
            PathingAlgorithm::Astar => {
                v = get_travel_costs(goal, &goal_view);
                &v
            }
            PathingAlgorithm::LpaStar => {
                pather_goal.update_tile_costs(&goal_view);
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
fn get_edge_points(tile_costs: &CostMapView, goal: Point) -> Vec<Point> {
    let width = tile_costs.width;
    let height = tile_costs.height;

    let mut points = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let p = Point { x: x, y: y };
            let n = tile_costs.get_predecessors(p).len();
            let is_edge = (n > 0) // needs at least one connection
                && match p {
                    p if p == goal => n > 0, // Goal is valid if at least one connected point
                    p if (p.x == width - 1 || p.x == 0) && (p.y == 0 || p.y == height - 1) => n < 2, // corner point, 2 edges
                    p if p.x == 0 || p.y == 0 => n < 3, // side point, 3 edges
                    p if p.x == width - 1 || p.y == height - 1 => n < 3, // side point, 3 edges
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
        let self_total = self
            .dist_to_start
            .checked_add(self.dist_to_goal)
            .unwrap_or(i32::MAX);
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
    let tile_costs = CostMap::from_world(&world);
    let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);

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
        let mut path = get_path(cur_loc, target_point, &cost_view).unwrap();
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
fn get_path(start: Point, end: Point, costs: &CostMapView) -> Option<Vec<Point>> {
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

        for (n, cost) in costs.get_successors(node) {
            let d = distance_matrix[n.y][n.x];

            // Only path over areas where we have cost data
            if d.is_none() {
                let new_cost = distance + cost;
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
    assert!(path.len() > 0); // Should never be called when a path doesn't exist
    return path;
}

/// Return the lowest travel cost matrix for all visible tiles if possible
///
/// None means no path is possible or there isn't tile information
fn get_travel_costs(start: Point, tile_costs: &CostMapView) -> Vec<Vec<i32>> {
    let width = tile_costs.width;
    let height = tile_costs.height;
    let mut travel_costs = vec![vec![i32::MAX; width]; height];

    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    travel_costs[start.y][start.x] = 0;

    while !queue.is_empty() {
        let (node, _) = queue.pop().unwrap();
        let distance = travel_costs[node.y][node.x];

        for (n, cost) in tile_costs.get_successors(node) {
            let d = travel_costs[n.y][n.x];

            let new_cost = distance.checked_add(cost).unwrap_or(i32::MAX);
            travel_costs[n.y][n.x] = min(d, new_cost);
            if new_cost < d {
                queue.push(n, Reverse(new_cost));
            }
        }
    }

    return travel_costs;
}

/// Prints the minimum cost to get to a given cell
pub fn system_print_tile_costs(world: &World) {
    let costs = CostMap::from_world(world);
    let cost_view = CostMapView::new(&costs, vec![EdgeType::Visible, EdgeType::Fog]);
    let mut output = vec![vec![i32::MAX; costs.width]; costs.height];

    for y in 0..costs.height {
        for x in 0..costs.width {
            let to = Point { x: x, y: y };

            let predecessors = cost_view.get_predecessors(to);
            output[to.y][to.x] = predecessors
                .iter()
                .map(|(_, c)| *c)
                .min()
                .unwrap_or(i32::MAX);
        }
    }

    for y in 0..costs.height {
        for x in 0..costs.width {
            let display_cost = match output[y][x] {
                i32::MAX => -1,
                cost => cost,
            };
            print! {"{}\t", display_cost}
        }
        println!("")
    }
}

/// Implementation for LPA*, based on:
/// https://en.wikipedia.org/wiki/Lifelong_Planning_A*
pub struct LpaStarPather {
    queue: PriorityQueue<Point, Reverse<LpaKey>>,
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
    let tile_costs = CostMap::from_world(world);
    let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);

    return LpaStarPather::new(start, goal, &cost_view);
}

pub fn get_goal_lpapather(world: &World) -> LpaStarPather {
    let start = get_start(world);
    let goal = get_goal(world);
    let tile_costs = CostMap::from_world(world);
    let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible, EdgeType::Fog]);

    return LpaStarPather::new(goal, start, &cost_view);
}

impl LpaStarPather {
    fn new(start: Point, goal: Point, tile_costs: &CostMapView) -> Self {
        let width = tile_costs.width;
        let height = tile_costs.height;

        let queue = PriorityQueue::new();

        // Using i32 max as placeholder for infinity
        let g = vec![vec![i32::MAX; width]; height];
        let rhs = vec![vec![i32::MAX; width]; height];

        let mut pather = Self {
            queue: queue,
            g,
            rhs,
            width,
            height,
            start,
            goal,
        };

        pather.rhs[start.y][start.x] = 0;
        pather.queue.push(start, pather.calculate_key(&start));

        pather.compute_shortest_path(tile_costs);

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

    fn compute_shortest_path(&mut self, tile_costs: &CostMapView) {
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
                    self.update_node(&s, tile_costs);
                }
            } else {
                self.g[node.y][node.x] = i32::MAX;
                self.update_node(&node, tile_costs);
                for s in get_neighbors(node, self.width, self.height) {
                    self.update_node(&s, tile_costs);
                }
            }
        }
    }

    /// Recalculates rhs for a node and removes it from the queue.
    /// If the node has become locally inconsistent, it is (re-)inserted into the queue with its new key.
    fn update_node(&mut self, p: &Point, tile_costs: &CostMapView) {
        if *p != self.start {
            self.rhs[p.y][p.x] = i32::MAX;
            for (n, cost) in tile_costs.get_predecessors(*p) {
                self.rhs[p.y][p.x] = min(
                    self.rhs[p.y][p.x],
                    // Ok to unwrap here, edges are guaranteed to be bi-directional, if not, there is a bug
                    self.get_g(n).checked_add(cost).unwrap_or(i32::MAX),
                )
            }
            self.queue.remove(p);
            if self.get_g(*p) != self.get_rhs(*p) {
                self.queue.push(*p, self.calculate_key(p));
            }
        }
    }

    fn update_tile_costs(&mut self, cost_view: &CostMapView) {
        // May need to switch back to tracking changed for performance, but for now, update all nodes
        for y in 0..cost_view.height {
            for x in 0..cost_view.width {
                let p = Point { x: x, y: y };
                self.update_node(&p, cost_view);
            }
        }

        self.compute_shortest_path(cost_view);
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
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, &cost_view);

        assert_eq!(pather.g, vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]);
    }

    #[test]
    fn test_lpa_no_update_cost() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 0, 0]]);
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let pather = LpaStarPather::new(start, goal, &cost_view);

        assert_eq!(
            pather.g,
            vec![vec![0, 11, 6], vec![1, 12, 5], vec![2, 3, 4]]
        );
    }

    #[test]
    fn test_lpa_update() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0; 3]; 3]);
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);
        let start = Point { x: 0, y: 0 };
        let goal = Point { x: 2, y: 2 };

        let mut pather = LpaStarPather::new(start, goal, &cost_view);

        assert_eq!(pather.g, vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]);

        // discover a wall
        let updated = CostMap::_from_vec(&vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 10, 0]]);
        let cost_view = CostMapView::new(&updated, vec![EdgeType::Visible]);

        pather.update_tile_costs(&cost_view);

        assert_eq!(
            pather.g,
            vec![vec![0, 11, 12], vec![1, 12, 13], vec![2, 13, 14]]
        );
    }

    #[test]
    fn find_path_no_cost() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0; 3]; 3]);
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);

        let path = get_path(Point { x: 0, y: 0 }, Point { x: 2, y: 2 }, &cost_view);
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
        let tile_costs = CostMap::_from_vec(&vec![vec![0; 3], vec![10, 10, 0], vec![0; 3]]);
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);

        let path = get_path(Point { x: 0, y: 0 }, Point { x: 2, y: 2 }, &cost_view);
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
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &cost_view);
        assert_eq!(
            travel_costs,
            vec![vec![0, 1, 2], vec![1, 2, 3], vec![2, 3, 4]]
        )
    }

    #[test]
    fn test_travel_cost_with_tile_costs() {
        let tile_costs = CostMap::_from_vec(&vec![vec![0, 10, 0], vec![0, 10, 0], vec![0, 0, 0]]);
        let cost_view = CostMapView::new(&tile_costs, vec![EdgeType::Visible]);
        let start = Point { x: 0, y: 0 };

        let travel_costs = get_travel_costs(start, &cost_view);
        assert_eq!(
            travel_costs,
            vec![vec![0, 11, 6], vec![1, 12, 5], vec![2, 3, 4]]
        )
    }
}
