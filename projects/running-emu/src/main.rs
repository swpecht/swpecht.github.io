use std::{collections::{VecDeque}, hash::Hash};

const WORLD_SIZE: usize = 15;

#[derive(Clone, Copy)]
struct World {
    costs: [Option<i8>; WORLD_SIZE * WORLD_SIZE],
    start: Point,
    goal: Point,
}

#[derive(PartialEq, Clone, Copy, Hash, Eq)]
struct Point {
    x: usize,
    y: usize
}

fn new_world() -> World {
    World { costs: [Some(0); WORLD_SIZE * WORLD_SIZE], start: Point {x: 0, y: 0}, goal: Point{x: 9, y: 9} }
}

fn main() {
    let world = new_world();

    print_world(world);
    println!("");
    find_path_bfs(world);
}

fn print_world(world: World) {
    for y in 0..WORLD_SIZE {
        for x in 0..WORLD_SIZE {
            let p = Point{x: x, y: y};

            if p == world.start {
                print!("S")
            } else if p == world.goal {
                print!("G")
            } else {
                print!("{}", world.costs[y * WORLD_SIZE + x].unwrap_or(-1))
            }
            print!("\t")            
        }
        println!("");
    }
}

/// Returns a vector of Points for the shortest path to the goal
fn find_path_bfs(world: World) -> Vec<Point> {
    let dmatrix = get_distance_matrix(world);
    let dist_world = World{costs: dmatrix, ..world};
    print_world(dist_world);

    return vec![world.start]
}

/// Return the distance to get to each point in the world from the starting point
fn get_distance_matrix(world: World) -> [Option<i8>; WORLD_SIZE * WORLD_SIZE] {
    let mut dmatrix: [Option<i8>; WORLD_SIZE * WORLD_SIZE] = [None; WORLD_SIZE * WORLD_SIZE];
    let mut queue: VecDeque<Point> = VecDeque::new();

    queue.push_back(world.start);
    let start = world.start;
    dmatrix[start.y * WORLD_SIZE + start.x] = Some(0);

    while !queue.is_empty() {
        let node = queue.pop_front().unwrap();
        if node == world.goal {
            break
        }

        let neighors = get_neighbors(node);
        let d = dmatrix[node.y * WORLD_SIZE + node.x].unwrap();
        for n in neighors {
            if dmatrix[n.y * WORLD_SIZE + n.x].is_none() {      
                dmatrix[n.y * WORLD_SIZE + n.x] = Some(d+1);
                queue.push_back(n);
            }
        }
        
    }

    return dmatrix
    
}

fn get_neighbors(point: Point) -> Vec<Point> {
    // Allow adjacent and diagonal movement
    let directions = [(-1, 0), (1, 0), (0, -1), (0, 1), (1, 1), (-1, -1), (1, -1), (-1, 1)];
    let mut neighbors: Vec<Point> = Vec::new();


    for d in directions {
        let candidate = Point {x: (point.x as i32 + d.0) as usize, y: (point.y as i32 + d.1) as usize};
        if candidate.x < WORLD_SIZE && candidate.y < WORLD_SIZE {
            neighbors.push(candidate);
        }
    }

    return neighbors
}