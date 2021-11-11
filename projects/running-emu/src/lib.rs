use std::{cmp::Reverse, hash::Hash};
use priority_queue::PriorityQueue;

#[derive(Clone)]
pub struct World {
    costs: Vec<i8>,
    map: String,
    width: usize,
    start: Point,
    goal: Point,
}

#[derive(PartialEq, Clone, Copy, Hash, Eq, Debug)]
pub struct Point {
    x: usize,
    y: usize
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
        let mut costs: Vec<i8> = vec![];
        let mut x = 0;
        let mut y = 0;
        let mut start = None;
        let mut goal = None;
        let mut width = None;

        for c in str_map.chars() {
            match c {
                '.' => {costs.push(0); x+=1},
                'S' => {costs.push(0); start = Some(Point{x: x, y: y}); x+=1}
                'G' => {costs.push(0); goal = Some(Point{x: x, y: y}); x+=1}
                'W' => {costs.push(10); x+=1} // Walls have cost of 10
                ' ' => {} // ignore white spaces
                '\n' => {if !width.is_none() && width.unwrap() != x {panic!("Error parsing map, rows vary in width")}; width = Some(x); x = 0; y += 1}
                _ => panic!("Error parsing map, invalid character: {}", c)
            }
        }
        
        if start.is_none() || goal.is_none() || width.is_none() {
            panic!("Error parsing map, S and G must be defined");
        }

        let cleaned_map = str_map.to_string().replace(" ", "");
        return World { costs: costs, start: start.unwrap(), goal: goal.unwrap(), width: width.unwrap(), map: cleaned_map};
    }

    
}

pub fn print_map(world: &World) {
    println!("{}", world.map)
}

pub fn print_path(path: &Vec<Point>, world: &World) {
    for y in 0.. (world.costs.len() / world.width) {
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

pub fn print_world(world: &World) {
    for y in 0.. (world.costs.len() / world.width) {
        for x in 0..world.width {
            let p = Point{x: x, y: y};

            if p == world.start {
                print!("S")
            } else if p == world.goal {
                print!("G")
            } else {
                print!("{}", world.costs[y * world.width + x])
            }
            print!("\t")            
        }
        println!("");
    }
}

pub fn print_cost_matrix(world: &World, cmatrix: &Vec<Option<i8>>) {
    for y in 0.. (world.costs.len() / world.width) {
        for x in 0..world.width {
            let p = Point{x: x, y: y};

            if p == world.start {
                print!("S")
            } else if p == world.goal {
                print!("G")
            } else {
                print!("{}", cmatrix[y * world.width + x].unwrap_or(-1))
            }
            print!("\t")            
        }
        println!("");
    }
}

#[cfg(test)]
mod tests {
    use crate::{Point, World, create_map, find_path_bfs};

    #[test]
    fn find_path_bfs_simple() {
        let map = 
        "S..\n...\n..G";
        let world = World::from_map(map);
        let path = find_path_bfs(&world);
        assert_eq!(path, vec![Point { x: 0, y: 0 }, Point { x: 0, y: 1 }, Point { x: 0, y: 2 }, Point { x: 1, y: 2 }, Point { x: 2, y: 2 }])
    }

    #[test]
    fn find_path_bfs_walls() {
        let map = 
        "S..
         WW.
         ..G";
        let world = World::from_map(map);
        let path = find_path_bfs(&world);
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

/// Returns a vector of Points for the shortest path to the goal
pub fn find_path_bfs(world: &World) -> Vec<Point> {
    let cmatrix = get_distance_matrix(&world);
    print_cost_matrix(world, &cmatrix);

    // Given the cost matrix, we can start at the goal and greedily follow the lowest cost
    // path back to the starting point to get an optimal path.

    let mut path = vec![world.goal];
    while path.last().unwrap().clone() != world.start {
        let p = path.last().unwrap();
        let neighbors = get_neighbors(world, *p);
        let (_, min) = neighbors.iter().filter(|p| !cmatrix[p.x + p.y * world.width].is_none()).enumerate().min_by(|a, b| cmatrix[a.1.x + a.1.y * world.width].cmp(&cmatrix[b.1.x + b.1.y * world.width])).unwrap();
        path.push(*min);
    }

    path.reverse();
    return path
}

/// Return the distance to get to each point in the world from the starting point
/// 
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
fn get_distance_matrix(world: &World) -> Vec<Option<i8>> {
    let mut dmatrix = vec![None; world.costs.len()];
    let mut queue: PriorityQueue<Point, Reverse<i8>> = PriorityQueue::new();

    // Initialize starting cost to 0
    queue.push(world.start, Reverse(0));
    let start = world.start;
    dmatrix[start.y * world.width + start.x] = Some(0);

    while !queue.is_empty() {
        let (node, cost) = queue.pop().unwrap();
        if node == world.goal {
            break
        }

        let neighors = get_neighbors(world, node);
        for n in neighors {
            let index = n.y * world.width + n.x;
            if dmatrix[index].is_none() {
                let new_cost = cost.0 + 1 + world.costs[index]; // Cost always increases by minimum of 1     
                dmatrix[index] = Some(new_cost);
                queue.push(n, Reverse(new_cost));
            }
        }
        
    }

    return dmatrix
    
}

fn get_neighbors(world: &World, point: Point) -> Vec<Point> {
    // Allow adjacent and diagonal movement
    // let directions = [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let directions = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    let mut neighbors: Vec<Point> = Vec::new();


    for d in directions {
        let candidate = Point {x: (point.x as i32 + d.0) as usize, y: (point.y as i32 + d.1) as usize};
        if candidate.x < world.width && candidate.y < (world.costs.len() / world.width) {
            neighbors.push(candidate);
        }
    }

    return neighbors
}