
use std::{cmp::Reverse, hash::Hash};
use priority_queue::PriorityQueue;

/// Represents the game world
#[derive(Clone)]
pub struct World {
    entities: Vec<Vec<Option<usize>>>, // helper data structure to keep track of entities with position
    entities_count: usize,
    pub width: usize,
    pub height: usize,
    pub position_components: Vec<Option<Point>>,
    pub sprite_components: Vec<Option<char>>,
    pub vis_components: Vec<Option<bool>>,
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
        let mut width = None;
        let mut tiles = vec![vec![]];

        // calculate width
        for c in str_map.chars() {
            match c {
                '.' | 'W' | 'S' | 'G' => {tiles[y].push(c); x+=1},
                ' ' => {}
                '\n' => {if !width.is_none() && width.unwrap() != x {panic!("Error parsing map, rows vary in width")}; width = Some(x); x = 0; y += 1; tiles.push(vec![])}
                _ => panic!("Error parsing map, invalid character: {}", c)
            }
        }


        let mut w = Self {entities_count: 0, entities: vec![vec![None; width.unwrap()]; y+1], width: width.unwrap(), height: y+1, position_components: Vec::new(), sprite_components: Vec::new(), vis_components: Vec::new()};

        for y in 0..tiles.len() {
            for x in 0.. tiles[0].len() {
                let c = tiles[y][x];
                match c {
                    '.' | 'W' | 'S' | 'G' => {w.parse_entities(c, Point{ x: x, y: y})},
                    ' ' | '\n' => {} // ignore white spaces and line breaks
                    _ => panic!("Error parsing map, invalid character: {}", c)
                }
            }
        }

         return w
    }

    fn parse_entities(&mut self, c: char, p: Point) {
        match c {
            'G' | 'S' => self.new_entity(Some(p), Some(c), Some(true)), // Goal and Start are visible to begin
            _ => self.new_entity(Some(p), Some(c), None), // All others must be found
        }
    }

    /// Returns tile at a given location or '.' if no entities present
    pub fn get_tile(&self, p: Point) -> char {
        let id = self.entities[p.y][p.x];
        
        match id {
            Some(id) => self.sprite_components[id].unwrap_or('.'),
            _ => '.',
        }
    }
    
    pub fn new_entity(&mut self, position: Option<Point>, sprite: Option<char>, vis: Option<bool>) {
        self.position_components.push(position);
        self.sprite_components.push(sprite);
        self.vis_components.push(vis);


        match position {
            Some(p) => self.entities[p.y][p.x] = Some(self.entities_count),
            _ => {}
        }

        self.entities_count += 1;
    }

    pub fn set_visible(&mut self, p: Point, vis: bool) {
        let id = self.entities[p.y][p.x];

        match id {
            Some(id) => self.vis_components[id] = Some(vis),
            _ => {},
        }        
    }
}

/// Point in the game world
#[derive(PartialEq, Clone, Copy, Hash, Eq, Debug)]
pub struct Point {
    pub x: usize,
    pub y: usize
}

/// Manages a view of the game world to explore given different policies.
pub struct AttackerAgent {
    cur_map: Vec<Vec<char>>,
    cur_costs: Vec<Vec<Option<i32>>>,
    queue: PriorityQueue<Point, Reverse<i32>>,
    start: Point,
    goal: Point,
}

impl AttackerAgent {
    pub fn new(world: &World) -> AttackerAgent {
 
        let mut start = None;
        let mut goal = None;

        // Find the proper entities and components for goal and start and update the map.
        let zip = world.position_components.iter().zip(world.sprite_components.iter());
        let pos_and_sprite = zip.filter_map(|(p, c): (&Option<Point>, &Option<char>)| {Some((p.as_ref()?, c.as_ref()?))});
        for (p, c) in pos_and_sprite {
            match c {
                'G' => goal = Some(p),
                'S' => start = Some(p),
                _ => {},
            }
        }

        let mut agent = AttackerAgent {cur_costs: vec![vec![None; world.width]; world.height],
            cur_map: vec![vec!['?'; world.width]; world.height],
            queue: PriorityQueue::new(), start: *start.unwrap(), goal: *goal.unwrap()};
        
        agent.update_map(*start.unwrap(), 'S');
        agent.update_map(*goal.unwrap(), 'G');
        
        
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

    pub fn get_tile(&self, p: Point) ->char {
        self.cur_map[p.y][p.x]
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
            if path.contains(&p) {
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
            print!("{}", agent.get_cost(p).unwrap_or(-1));
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
        let mut world = World::from_map(map);
        let mut agent = AttackerAgent::new(&world);
        let (path, _) = find_path_bfs(&mut world, &mut agent);
        assert_eq!(path, vec![Point { x: 0, y: 0 }, Point { x: 0, y: 1 }, Point { x: 0, y: 2 }, Point { x: 1, y: 2 }, Point { x: 2, y: 2 }])
    }

    #[test]
    fn find_path_bfs_walls() {
        let map = 
        "S..
         WW.
         ..G";
        let mut world = World::from_map(map);
        let mut agent = AttackerAgent::new(&world);
        let (path, _) = find_path_bfs(&mut world, &mut agent);
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
pub fn find_path_bfs(world: &mut World, agent: &mut AttackerAgent) -> (Vec<Point>, i32) {
    let mut steps = 0;
    while !attacker_system_update(world, agent) {
        steps += 1;
    }

    let path = get_path_from_agent(world, agent);
    return (path, steps)
}

/// Return the path to the goal. Requires a fully populated cost matrix.
/// 
/// Given the cost matrix, we can start at the goal and greedily follow the lowest cost
///  path back to the starting point to get an optimal path.
pub fn get_path_from_agent(world: &World, agent: &AttackerAgent) -> Vec<Point> {
    let mut path = vec![agent.goal];
    while path.last().unwrap().clone() != agent.start {
        let p = path.last().unwrap();
        let neighbors = get_neighbors(world, *p);
        let (_, min) = neighbors.iter().filter(|p| !agent.get_cost(**p).is_none()).enumerate().min_by(|a, b| agent.get_cost(*a.1).cmp(&agent.get_cost(*b.1))).unwrap();
        path.push(*min);
    }

    path.reverse();
    return path;
}

/// Update the attacker AI system. Returns True if have reached the goal.
/// 
/// Populate the cost matrix for a given attacher agent with the distance from each point in the world to the start.
/// 
/// The lowest cost space is always explored next rather than traditional breadth first search.
/// This ensures that tiles costs always represent the 'cheapest' way to get to the tile.
pub fn attacker_system_update(world: &mut World, agent: &mut AttackerAgent) -> bool {
    if agent.queue.is_empty() {
        // Initialize starting cost to 0
        agent.queue.push(agent.start, Reverse(0));
        let start = agent.start;
        agent.update_cost(start, 0);
    }

    let (node, _) = agent.queue.pop().unwrap();
        let cost = agent.get_cost(node).unwrap();
        if node == agent.goal {
            return true
        }

        let neighors = get_neighbors(world, node);
        for n in neighors {
            let c = agent.get_cost(n);
            if c.is_none() {     
                let new_cost = cost + 1 + get_tile_cost(world.get_tile(n)); // Cost always increases by minimum of 1     
                agent.update_cost(n, new_cost);
                // Distance heuristic for A*
                let dist_heuristic = (agent.goal.x as i32 - n.x as i32).abs() + (agent.goal.y as i32 - n.y as i32).abs();
                agent.queue.push(n, Reverse(dist_heuristic + new_cost));
                agent.update_map(n, world.get_tile(n));

                world.set_visible(n, true); // Set tile as visible
            }
        }

    return false
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