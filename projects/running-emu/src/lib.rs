use std::{
    cmp::{max, Reverse},
    collections::HashMap,
};

use crossterm::style::Color;
use hecs::World;
use itertools::Itertools;
use priority_queue::PriorityQueue;

use crate::spatial::{get_entity, Point};

pub mod spatial;

pub struct Position(pub Point);
pub struct Sprite(pub char);
pub struct Visibility(pub bool);
/// Determine if the background for an entity should be highlighted
pub struct BackgroundHighlight(pub Color);
/// How far an entity can see.
pub struct Vision(pub usize);
pub struct TargetLocation(pub Option<Point>);

pub struct Agent;

pub fn print_cost_matrix(world: &World, agent: &AttackerAgent) {
    let tile_costs = get_tiles_costs(world);
    let travel_costs = get_travel_costs(agent.start, &tile_costs);

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

    use crate::{create_map, get_path, get_travel_costs, Point};

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
    start: Point,
    goal: Point,
    next_target: Option<Point>, // Intermediate target to navigate to
    is_visited: HashMap<Point, bool>,
}

impl AttackerAgent {
    pub fn new(world: &World) -> AttackerAgent {
        let mut start = None;
        let mut goal = None;

        // Find the proper entities and components for goal and start and update the map.
        for (_, (p, c)) in world.query::<(&Position, &Sprite)>().iter() {
            match c.0 {
                'G' => goal = Some(p.0),
                'S' => start = Some(p.0),
                _ => {}
            }
        }

        let agent = AttackerAgent {
            start: start.unwrap(),
            goal: goal.unwrap(),
            next_target: None,
            is_visited: HashMap::new(),
        };

        return agent;
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

fn get_tiles_costs(world: &World) -> Vec<Vec<Option<i32>>> {
    // Constrcut the cost matrix from currently visible tiles
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

/// Move agents that have a target location.
///
/// This system handles the pathfinding part of the AI.
pub fn system_pathing(world: &mut World) {
    let tile_costs = get_tiles_costs(world);

    for (_, (pos, target)) in world.query_mut::<(&mut Position, &mut TargetLocation)>() {
        if target.0.is_none() {
            continue;
        }

        let path = get_path(pos.0, target.0.unwrap(), &tile_costs).unwrap();
        pos.0 = path[1];
    }
}

/// Identify where agents should move next to explore.
///
/// This system sets target locations and handles route highlighting.
///
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
pub fn system_ai(world: &mut World, agent: &mut AttackerAgent) -> bool {
    let agent_ids = world.query_mut::<&Agent>().into_iter().collect_vec();
    let agent_id = agent_ids[0].0; // Since only 1 agent

    let cur_loc = world.get::<Position>(agent_id).unwrap().0;
    agent.is_visited.insert(cur_loc, true);

    let tile_costs = get_tiles_costs(world);
    let travel_costs = get_travel_costs(agent.start, &tile_costs);

    // explore(world, agent, cur_loc, &tile_costs);

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
        let mut goal_dist_costs = vec![vec![Some(1); max_p.x]; max_p.y];
        for y in 0..max_p.y {
            for x in 0..max_p.x {
                goal_dist_costs[y][x] = Some(tile_costs[y][x].unwrap_or(0))
            }
        }

        for y in 0..max_p.y {
            for x in 0..max_p.x {
                let p = Point { x: x, y: y };
                match travel_costs[p.y][p.x] {
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
        world
            .insert_one(agent_id, TargetLocation(Some(min_p)))
            .unwrap();
    }

    // Highlight the target path
    let path = get_path(cur_loc, agent.next_target.unwrap(), &tile_costs).unwrap();
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
    return false;
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
