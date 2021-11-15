
use std::{cmp::Reverse, hash::Hash, fmt};
use priority_queue::PriorityQueue;

/// Represents the game world
#[derive(Clone)]
pub struct World {
    tiles: Vec<Vec<char>>,
    width: usize,
    height: usize,
    start: Point,
    goal: Point,
}

/// Point in the game world
#[derive(PartialEq, Clone, Copy, Hash, Eq, Debug)]
pub struct Point {
    x: usize,
    y: usize
}

/// Manages a view of the game world to explore given different policies.
pub struct AttackerAgent {
    cur_map: Vec<Vec<char>>,
    cur_costs: Vec<Vec<Option<i32>>>,
    queue: PriorityQueue<Point, Reverse<i32>>,
}

impl AttackerAgent {
    pub fn new(world: &World) -> AttackerAgent {
 
        let mut agent = AttackerAgent {cur_costs: vec![vec![None; world.width]; world.height],
             cur_map: vec![vec!['?'; world.width]; world.height],
            queue: PriorityQueue::new()};
        agent.update_map(world.goal, 'G');
        agent.update_map(world.start, 'S');
        
        return agent
    }

    fn update_map(&mut self, p: Point, tile: char) {
        self.cur_map[p.y][p.x] = tile
    }

    fn update_cost(&mut self, p: Point, cost: i32) {
        self.cur_costs[p.y][p.x] = Some(cost)
    }

    fn get_cost(&self, p: Point) -> Option<i32> {
        self.cur_costs[p.y][p.x]
    }
}


impl World {
    pub fn new() -> World {
        return World::from_map("S..\n...\n..G")
    }

    /// Create a world from a string representation of a map.
    /// 
    /// Maps are a series of characters with \\n representing new lines.
    /// All of the rows on a map must have the same number of characters.
    /// As an example, a simple map would be:
    /// S..
    /// .W.
    /// ..G
    /// Where S represents the start, W a wall, and G the goal.
    pub fn from_map(str_map: &str) -> World {
        let mut x = 0;
        let mut y = 0;
        let mut start = None;
        let mut goal = None;
        let mut width = None;
        let mut tiles = vec![vec![]];

        for c in str_map.chars() {
            match c {
                '.' | 'W' => {x+=1},
                'S' => {start = Some(Point{x: x, y: y}); x+=1}
                'G' => {goal = Some(Point{x: x, y: y}); x+=1}
                ' ' => {} // ignore white spaces
                '\n' => {if !width.is_none() && width.unwrap() != x {panic!("Error parsing map, rows vary in width")}; width = Some(x); x = 0; y += 1; tiles.push(vec![])}
                _ => panic!("Error parsing map, invalid character: {}", c)
            }

            // populate tiles
            match c {
                '.' | 'W' | 'S' | 'G' => tiles[y].push(c),
                _ => {} // ignore all other characters
            }
        }
        
        if start.is_none() || goal.is_none() || width.is_none() {
            panic!("Error parsing map, S and G must be defined");
        }

        return World {tiles: tiles, width: width.unwrap(), height: y+1, start: start.unwrap(), goal: goal.unwrap()};
    }

    pub fn get_tile(&self, p: Point) -> char {
        self.tiles[p.y][p.x]
    }
    
}

impl fmt::Display for World {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for y in 0.. (self.height) {
            for x in 0..self.width {
                write!(f, "{}", self.get_tile(Point{x: x, y:y}))?;
            }
            writeln!(f, "")?;
        }

        Ok(())
    }
}

fn get_tile_cost(tile: char) -> i32 {
    match tile {
        '.' => 0,
        'S' => 0,
        'G' => 0,
        'W' => 10, // Walls have cost of 10
        _ => panic!("Error parsing map, invalid character: {}", &tile)
    }
}

pub fn print_path(path: &Vec<Point>, world: &World) {
    for y in 0.. (world.height) {
        for x in 0..world.width {
            let p = Point { x: x, y: y};
            if p == world.start {
                print!("S")
            } else if p == world.goal {
                print!("G")
            } else if path.contains(&p) {
                print!("#")
            } else {
                print!{"."}
            }
        }
        println!("")
    }
}

pub fn print_cost_matrix(world: &World, agent: &AttackerAgent) {
    for y in 0.. (world.height) {
        for x in 0..world.width {
            let p = Point{x: x, y: y};

            if p == world.start {
                print!("S")
            } else if p == world.goal {
                print!("G")
            } else {
                print!("{}", agent.get_cost(p).unwrap_or(-1))
            }
            print!("\t")            
        }
        println!("");
    }
}

#[cfg(test)]
mod tests {
    use crate::{AttackerAgent, Point, World, create_map, find_path_bfs};

    #[test]
    fn find_path_bfs_simple() {
        let map = 
        "S..\n...\n..G";
        let world = World::from_map(map);
        let mut agent = AttackerAgent::new(&world);
        let (path, _) = find_path_bfs(&world, &mut agent);
        assert_eq!(path, vec![Point { x: 0, y: 0 }, Point { x: 0, y: 1 }, Point { x: 0, y: 2 }, Point { x: 1, y: 2 }, Point { x: 2, y: 2 }])
    }

    #[test]
    fn find_path_bfs_walls() {
        let map = 
        "S..
         WW.
         ..G";
        let world = World::from_map(map);
        let mut agent = AttackerAgent::new(&world);
        let (path, _) = find_path_bfs(&world, &mut agent);
        assert_eq!(path, vec![Point { x: 0, y: 0 }, Point { x: 1, y: 0 }, Point { x: 2, y: 0 }, Point { x: 2, y: 1 }, Point { x: 2, y: 2 }])
    }

    #[test]
    fn create_map_empty() {
        let map = create_map(3);
        assert_eq!(map, "S..\n...\n..G")
    }
}

pub fn create_map(size: usize) -> String {
    let mut map = String::from("");

    for y in 0..size{
        for x in 0..size {
            let c = match (x, y) {
                (0, 0) => 'S',
                (x, y) if x == size - 1 && y == size - 1 => 'G',
                _ => '.'
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
pub fn find_path_bfs(world: &World, agent: &mut AttackerAgent) -> (Vec<Point>, i32) {
    let steps = populate_cost_matrix(&world, agent);

    // Given the cost matrix, we can start at the goal and greedily follow the lowest cost
    // path back to the starting point to get an optimal path.
    let mut path = vec![world.goal];
    while path.last().unwrap().clone() != world.start {
        let p = path.last().unwrap();
        let neighbors = get_neighbors(world, *p);
        let (_, min) = neighbors.iter().filter(|p| !agent.get_cost(**p).is_none()).enumerate().min_by(|a, b| agent.get_cost(*a.1).cmp(&agent.get_cost(*b.1))).unwrap();
        path.push(*min);
    }

    path.reverse();
    return (path, steps)
}

/// Populate the cost matrix for a given attacher agent with the distance from each point in the world to the start.
/// 
/// Return the number of steps to populate the matrix.
/// 
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
fn populate_cost_matrix(world: &World, agent: &mut AttackerAgent) -> i32 {
    // Initialize starting cost to 0
    agent.queue.push(world.start, Reverse(0));
    let start = world.start;
    agent.update_cost(start, 0);

    let mut num_steps = 0;

    while !agent.queue.is_empty() {
        let (node, _) = agent.queue.pop().unwrap();
        let cost = agent.get_cost(node).unwrap();
        if node == world.goal {
            break
        }

        let neighors = get_neighbors(world, node);
        for n in neighors {
            let c = agent.get_cost(n);
            if c.is_none() {     
                let new_cost = cost + 1 + get_tile_cost(world.get_tile(n)); // Cost always increases by minimum of 1     
                agent.update_cost(n, new_cost);
                // Distance heuristic for A*
                let dist_heuristic = (world.goal.x as i32 - n.x as i32).abs() + (world.goal.y as i32 - n.y as i32).abs();
                agent.queue.push(n, Reverse(dist_heuristic + new_cost));
            }
        }
        num_steps += 1;
    }

    return num_steps;    
}

fn get_neighbors(world: &World, point: Point) -> Vec<Point> {
    // Allow adjacent and diagonal movement
    // let directions = [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let directions = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    let mut neighbors: Vec<Point> = Vec::new();


    for d in directions {
        let candidate = Point {x: (point.x as i32 + d.0) as usize, y: (point.y as i32 + d.1) as usize};
        if candidate.x < world.width && candidate.y < (world.height) {
            neighbors.push(candidate);
        }
    }

    return neighbors
}