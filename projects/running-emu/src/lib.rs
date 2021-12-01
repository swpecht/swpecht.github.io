use std::{
    cmp::{max, Reverse},
    collections::HashMap,
};

use crossterm::style::Color;
use hecs::{Entity, World};
use priority_queue::PriorityQueue;
use spatial::get_tile;

use crate::spatial::{get_entity, Point};

pub mod spatial;

pub struct Position(pub Point);
pub struct Sprite(pub char);
pub struct Visibility(pub bool);
/// Determine if the background for an entity should be highlighted
pub struct BackgroundHighlight(pub Color);
/// How far an entity can see.
pub struct Vision(pub usize);
pub struct Velocity(pub i32, pub i32);

pub fn print_cost_matrix(world: &World, agent: &AttackerAgent) {
    let max_p = get_max_point(world);
    for y in 0..max_p.y {
        for x in 0..max_p.x {
            let p = Point { x: x, y: y };
            print!("{}", agent.get_cost(p).unwrap_or(-1));
            print!("\t")
        }
        println!("");
    }
}

/// Returns Point representing the bottom right corner + 1. Or (1, 1) if no entities.
///
/// Calculated based on entity locations
pub fn get_max_point(world: &World) -> Point {
    let mut max_x = 0;
    let mut max_y = 0;
    for (_, p) in world.query::<&Position>().iter() {
        max_x = max(max_x, p.0.x);
        max_y = max(max_y, p.0.y);
    }

    return Point {
        x: max_x + 1,
        y: max_y + 1,
    };
}

#[cfg(test)]
mod tests {
    use std::vec;

    use crate::{create_map, get_path, Point};

    #[test]
    fn create_map_empty() {
        let map = create_map(3);
        assert_eq!(map, "S..\n...\n..G")
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
}

pub fn create_map(size: usize) -> String {
    let mut map = String::from("");

    for y in 0..size {
        for x in 0..size {
            let c = match (x, y) {
                (0, 0) => 'S',
                (x, y) if x == size - 1 && y == size - 1 => 'G',
                _ => '.',
            };
            map.push(c);
        }
        if y < size - 1 {
            map.push('\n')
        }
    }

    return map;
}

/// Returns a vector of Points for the shortest path to the goal and the number of steps to calculate
pub fn find_path_bfs(world: &mut World, agent: &mut AttackerAgent) -> (Vec<Point>, i32) {
    let mut steps = 0;
    while !attacker_system_update(world, agent) {
        steps += 1;
    }

    let path = get_path_from_distances(agent.start, agent.goal, &agent.cur_costs);
    return (path, steps);
}

/// Return the path to the goal. Requires a fully populated distance matrix.
///
/// Given the cost matrix, we can start at the goal and greedily follow the lowest cost
///  path back to the starting point to get an optimal path.
pub fn get_path_from_distances(
    start: Point,
    end: Point,
    distance_matrix: &Vec<Vec<Option<i32>>>,
) -> Vec<Point> {
    let mut path = vec![end];
    let width = distance_matrix[0].len();
    let height = distance_matrix.len();

    while path.last().unwrap().clone() != start {
        let p = path.last().unwrap();
        let neighbors = get_neighbors(*p, width, height);
        let (_, min) = neighbors
            .iter()
            .filter(|p| !distance_matrix[p.y][p.x].is_none())
            .enumerate()
            .min_by(|a, b| distance_matrix[a.1.y][a.1.x].cmp(&distance_matrix[b.1.y][b.1.x]))
            .unwrap();
        path.push(*min);
    }

    path.reverse();
    return path;
}

/// Manages a view of the game world to explore given different policies.
pub struct AttackerAgent {
    cur_costs: Vec<Vec<Option<i32>>>,
    start: Point,
    goal: Point,
    agend_id: Option<Entity>,   // Entity id for the movable agent
    next_target: Option<Point>, // Intermediate target to navigate to
    is_visited: HashMap<Point, bool>,
}

impl AttackerAgent {
    pub fn new(world: &World) -> AttackerAgent {
        let mut start = None;
        let mut goal = None;
        let mut agent_id = None;
        let mut agent_start = None;

        // Find the proper entities and components for goal and start and update the map.
        for (_, (p, c)) in world.query::<(&Position, &Sprite)>().iter() {
            match c.0 {
                'G' => goal = Some(p.0),
                'S' => start = Some(p.0),
                '@' => {
                    agent_id = get_entity(world, p.0);
                    agent_start = Some(p.0)
                }
                _ => {}
            }
        }

        let max_p = get_max_point(world);
        let mut agent = AttackerAgent {
            cur_costs: vec![vec![None; max_p.x]; max_p.y],
            start: start.unwrap(),
            goal: goal.unwrap(),
            agend_id: agent_id,
            next_target: None,
            is_visited: HashMap::new(),
        };

        // Update the agent starting spot cost
        match agent_start {
            Some(p) => agent.update_cost(p, 0),
            _ => {}
        }
        return agent;
    }

    fn update_cost(&mut self, p: Point, cost: i32) {
        self.cur_costs[p.y][p.x] = Some(cost)
    }

    fn get_cost(&self, p: Point) -> Option<i32> {
        self.cur_costs[p.y][p.x]
    }
}

fn get_tile_cost(tile: char) -> i32 {
    match tile {
        '.' => 0,
        'S' => 0,
        'G' => 0,
        '@' => 0,
        'W' => 50, // Walls have cost of 10
        _ => panic!("Error parsing map, invalid character: {}", &tile),
    }
}

/// Update the attacker AI system. Returns True if have reached the goal.
///
/// Two phases to the attacker system:
///     1. Identify the next goal location we want to explore
///     2. Indentify path to that goal location
/// These two phases will continu on repeat until the final goal is found.
///
/// Populate the cost matrix for a given attacher agent with the distance from each point in the world to the start.
///
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
pub fn attacker_system_update(world: &mut World, agent: &mut AttackerAgent) -> bool {
    let cur_loc = world.get::<Position>(agent.agend_id.unwrap()).unwrap().0;

    agent.is_visited.insert(cur_loc, true);
    explore(world, agent, cur_loc);

    if cur_loc == agent.goal {
        return true; // Found the goal
    }

    // Generate the next target if we're there or don't have a goal.
    //
    // The next target is chosen by constructing a matrix of scores for all possible explored locations and choosing the minimum
    // An explored location will not be chosen. The scores for squares have the following form:
    //  Score(point) = cost to get there from start + distance from the agent + cost to get to goal assuming un-explored squares have only travel cost
    let max_p = get_max_point(world);
    if agent.next_target.is_none() || cur_loc == agent.next_target.unwrap() {
        let mut candidate_matrix = vec![None; max_p.x * max_p.y];
        // Create a cost matrix where unknown tiles have a cost of 1
        let tile_costs = get_cost_matrix(world);
        let mut goal_dist_costs =
            vec![vec![Some(1); agent.cur_costs[0].len()]; agent.cur_costs.len()];
        for y in 0..agent.cur_costs.len() {
            for x in 0..agent.cur_costs[0].len() {
                goal_dist_costs[y][x] = Some(tile_costs[y][x].unwrap_or(0))
            }
        }

        for y in 0..max_p.y {
            for x in 0..max_p.x {
                let p = Point { x: x, y: y };
                match agent.get_cost(p) {
                    Some(cost) => {
                        let goal_dist =
                            get_path(p, agent.goal, &goal_dist_costs).unwrap().len() as i32;
                        let agent_dist = p.dist(&cur_loc);
                        candidate_matrix[x + y * max_p.x] = Some(cost + goal_dist + agent_dist)
                    }
                    _ => {}
                };
                // Don't choose a location previously visited
                if agent.is_visited.contains_key(&p) {
                    candidate_matrix[x + y * max_p.x] = None;
                }
            }
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
        println!("{:?}", min_p);
        agent.next_target = Some(min_p);
    }

    if cur_loc != agent.next_target.unwrap() {
        let path = get_path(cur_loc, agent.next_target.unwrap(), &get_cost_matrix(world)).unwrap();
        // Move the explorer '@'
        match agent.agend_id {
            Some(id) => {
                world
                    .insert_one(
                        id,
                        Velocity(
                            path[1].x as i32 - cur_loc.x as i32,
                            path[1].y as i32 - cur_loc.y as i32,
                        ),
                    )
                    .unwrap();
            }
            _ => {}
        }

        for p in path {
            let e = get_entity(world, p);
            let color = match p {
                p if p == agent.next_target.unwrap() => Color::Green,
                _ => Color::Blue,
            };

            match e {
                Some(e) => {
                    world.insert_one(e, BackgroundHighlight(color)).unwrap();
                }
                _ => {}
            }
        }
    }

    explore(world, agent, cur_loc);

    return false;
}

/// Returns a cost matrix representing the cost of visible tiles.
fn get_cost_matrix(world: &mut World) -> Vec<Vec<Option<i32>>> {
    let max_p = get_max_point(world);
    let mut costs = vec![vec![None; max_p.x]; max_p.y];

    for (_, (p, c, v)) in world.query_mut::<(&Position, &Sprite, &Visibility)>() {
        if v.0 {
            costs[p.0.y][p.0.x] = Some(get_tile_cost(c.0));
        }
    }

    return costs;
}

// Explore a given point for the agent, and update the move state
pub fn explore(world: &mut World, agent: &mut AttackerAgent, p: Point) {
    let max_p = get_max_point(world);
    let neighors = get_neighbors(p, max_p.x, max_p.y);
    let cost = agent.get_cost(p).unwrap();
    for n in neighors {
        let c = agent.get_cost(n);
        let new_cost = cost + 1 + get_tile_cost(get_tile(world, n)); // Cost always increases by minimum of 1

        // Update if we have no cost or found a lower cost way to get here
        if c.is_none() || c.unwrap() > new_cost {
            agent.update_cost(n, new_cost);
        }
    }
}

/// Find a path between 2 arbitraty points if it exists
///
/// Only navigates through known costs
fn get_path(start: Point, end: Point, cost_matrix: &Vec<Vec<Option<i32>>>) -> Option<Vec<Point>> {
    let mut queue: PriorityQueue<Point, Reverse<i32>> = PriorityQueue::new();
    queue.push(start, Reverse(0));
    let width = cost_matrix[0].len();
    let height = cost_matrix.len();
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
            if d.is_none() && cost_matrix[n.y][n.x].is_some() {
                let new_cost = distance + 1 + cost_matrix[n.y][n.x].unwrap(); // Cost always increases by minimum of 1
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
