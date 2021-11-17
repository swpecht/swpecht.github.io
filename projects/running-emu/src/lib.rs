use crossterm::style::Color;
use priority_queue::PriorityQueue;
use std::{
    cell::{Ref, RefCell, RefMut},
    cmp::Reverse,
    collections::HashMap,
    hash::Hash,
};

/// Represents the game world
pub struct World {
    entities_count: usize,
    pub width: usize,
    pub height: usize,
    component_vecs: Vec<Box<dyn ComponentVec>>,
}

impl World {
    pub fn new() -> World {
        return World::from_map("S..\n...\n..G");
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
                '.' | 'W' | 'S' | 'G' | '@' => {
                    tiles[y].push(c);
                    x += 1
                }
                ' ' => {}
                '\n' => {
                    if !width.is_none() && width.unwrap() != x {
                        panic!("Error parsing map, rows vary in width")
                    };
                    width = Some(x);
                    x = 0;
                    y += 1;
                    tiles.push(vec![])
                }
                _ => panic!("Error parsing map, invalid character: {}", c),
            }
        }

        let mut w = Self {
            entities_count: 0,
            width: width.unwrap(),
            height: y + 1,
            component_vecs: Vec::new(),
        };

        for y in 0..tiles.len() {
            for x in 0..tiles[0].len() {
                let c = tiles[y][x];
                w.parse_entities(c, Point { x: x, y: y });
            }
        }

        return w;
    }

    fn parse_entities(&mut self, c: char, p: Point) {
        let id = self.new_entity();
        self.add_component_to_entity(id, Position(p));
        self.add_component_to_entity(id, Sprite(c));
        match c {
            'G' | 'S' | '@' => self.add_component_to_entity(id, Visibility(true)), // Goal and Start are visible to begin
            _ => {} // All others must be found
        }

        if c == '@' {
            // Create a background entity to represent the tile
            let bg = self.new_entity();
            self.add_component_to_entity(bg, Position(p));
            self.add_component_to_entity(bg, Sprite('.'));
            self.set_visible(p, true);
        }
    }

    /// Returns tile at a given location or '.' if no entities present
    pub fn get_tile(&self, p: Point) -> char {
        let id = self.get_entity(p);
        let data = self.borrow_component_vec::<Sprite>();

        match (id, data) {
            (Some(id), Some(data)) => data[id].as_ref().unwrap_or(&Sprite('.')).0,
            _ => '.',
        }
    }

    /// Return the id of the new entity
    pub fn new_entity(&mut self) -> usize {
        let entity_id = self.entities_count;
        for component_vec in self.component_vecs.iter_mut() {
            component_vec.push_none();
        }
        self.entities_count += 1;
        entity_id
    }

    pub fn add_component_to_entity<ComponentType: 'static>(
        &mut self,
        entity: usize,
        component: ComponentType,
    ) {
        for component_vec in self.component_vecs.iter_mut() {
            // The `downcast_mut` type here is changed to `RefCell<Vec<Option<ComponentType>>`
            if let Some(component_vec) = component_vec
                .as_any_mut()
                .downcast_mut::<RefCell<Vec<Option<ComponentType>>>>()
            {
                // add a `get_mut` here. Once again `get_mut` bypasses
                // `RefCell`'s runtime checks if accessing through a `&mut` reference.
                component_vec.get_mut()[entity] = Some(component);
                return;
            }
        }

        let mut new_component_vec: Vec<Option<ComponentType>> =
            Vec::with_capacity(self.entities_count);

        for _ in 0..self.entities_count {
            new_component_vec.push(None);
        }

        new_component_vec[entity] = Some(component);

        // Here we create a `RefCell` before inserting into `component_vecs`
        self.component_vecs
            .push(Box::new(RefCell::new(new_component_vec)));
    }

    pub fn borrow_mut_component_vec<ComponentType: 'static>(
        &self,
    ) -> Option<RefMut<Vec<Option<ComponentType>>>> {
        for component_vec in self.component_vecs.iter() {
            if let Some(component_vec) = component_vec
                .as_any()
                .downcast_ref::<RefCell<Vec<Option<ComponentType>>>>()
            {
                // Here we use `borrow_mut`.
                // If this `RefCell` is already borrowed from this will panic.
                return Some(component_vec.borrow_mut());
            }
        }
        None
    }

    pub fn borrow_component_vec<ComponentType: 'static>(
        &self,
    ) -> Option<Ref<Vec<Option<ComponentType>>>> {
        for component_vec in self.component_vecs.iter() {
            if let Some(component_vec) = component_vec
                .as_any()
                .downcast_ref::<RefCell<Vec<Option<ComponentType>>>>()
            {
                return Some(component_vec.borrow());
            }
        }
        None
    }

    pub fn set_visible(&mut self, p: Point, vis: bool) {
        let id = self.get_entity(p);

        match id {
            Some(id) => self.add_component_to_entity(id, Visibility(vis)),
            _ => {}
        }
    }

    /// Return the entity at a given point if one exists
    pub fn get_entity(&self, p: Point) -> Option<usize> {
        let positions = self.borrow_component_vec::<Position>().unwrap();
        for i in 0..positions.len() {
            match &positions[i] {
                Some(candidate) if candidate.0 == p => return Some(i),
                _ => {}
            }
        }

        return None;
    }
}

trait ComponentVec {
    fn push_none(&mut self);
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<T: 'static> ComponentVec for RefCell<Vec<Option<T>>> {
    fn push_none(&mut self) {
        self.get_mut().push(None);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self as &dyn std::any::Any
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self as &mut dyn std::any::Any
    }
}

pub struct Position(pub Point);
pub struct Sprite(pub char);
pub struct Visibility(pub bool);
/// Determine if the background for an entity should be highlighted
pub struct BackgroundHighlight(pub Color);

/// Point in the game world
#[derive(PartialEq, Clone, Copy, Hash, Eq, Debug)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Point {
    /// Returns taxicab distance between points
    pub fn dist(&self, p: &Point) -> i32 {
        return (self.x as i32 - p.x as i32).abs() + (self.y as i32 - p.y as i32).abs();
    }
}

pub fn print_path(path: &Vec<Point>, world: &World) {
    for y in 0..(world.height) {
        for x in 0..world.width {
            let p = Point { x: x, y: y };
            if path.contains(&p) {
                print!("#")
            } else {
                print! {"."}
            }
        }
        println!("")
    }
}

pub fn print_cost_matrix(world: &World, agent: &AttackerAgent) {
    for y in 0..(world.height) {
        for x in 0..world.width {
            let p = Point { x: x, y: y };
            print!("{}", agent.get_cost(p).unwrap_or(-1));
            print!("\t")
        }
        println!("");
    }
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
    agend_id: Option<usize>,    // Entity id for the movable agent
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
        let positions = world.borrow_component_vec::<Position>().unwrap();
        let sprites = world.borrow_component_vec::<Sprite>().unwrap();
        let zip = positions.iter().zip(sprites.iter());
        let pos_and_sprite = zip.filter_map(|(p, c): (&Option<Position>, &Option<Sprite>)| {
            Some((p.as_ref()?, c.as_ref()?))
        });
        for (p, c) in pos_and_sprite {
            match c.0 {
                'G' => goal = Some(p.0),
                'S' => start = Some(p.0),
                '@' => {
                    agent_id = world.get_entity(p.0);
                    agent_start = Some(p.0)
                }
                _ => {}
            }
        }

        let mut agent = AttackerAgent {
            cur_costs: vec![vec![None; world.width]; world.height],
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
    let mut cur_loc = world.borrow_mut_component_vec::<Position>().unwrap()
        [agent.agend_id.unwrap()]
    .as_ref()
    .unwrap()
    .0;

    agent.is_visited.insert(cur_loc, true);
    explore(world, agent, cur_loc);

    if cur_loc == agent.goal {
        return true; // Found the goal
    }

    // Generate the next target if we're there or don't have a goal
    if agent.next_target.is_none() || cur_loc == agent.next_target.unwrap() {
        let mut candidate_matrix = vec![None; world.width * world.height];
        for y in 0..world.height {
            for x in 0..world.width {
                let p = Point { x: x, y: y };
                match agent.get_cost(p) {
                    Some(c) => {
                        candidate_matrix[x + y * world.width] =
                            Some(c + p.dist(&agent.goal) + p.dist(&cur_loc))
                    }
                    _ => {}
                };
                // Don't choose a location previously visited
                if agent.is_visited.contains_key(&p) {
                    candidate_matrix[x + y * world.width] = None;
                }
            }
        }

        let min_val = candidate_matrix
            .iter()
            .filter_map(|c| c.as_ref())
            .min()
            .unwrap();
        let mut min_p = Point { x: 0, y: 0 };
        for y in 0..world.height {
            for x in 0..world.width {
                if candidate_matrix[x + y * world.width] == Some(*min_val) {
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
                world.add_component_to_entity(id, Position(path[1]));
                cur_loc = path[1]
            }
            _ => {}
        }

        for p in path {
            let e = world.get_entity(p);
            let color = match p {
                p if p == agent.next_target.unwrap() => Color::Green,
                _ => Color::Blue,
            };

            match e {
                Some(e) => world.add_component_to_entity(e, BackgroundHighlight(color)),
                _ => {}
            }
        }
    }

    explore(world, agent, cur_loc);

    return false;
}

/// Returns a cost matrix representing the cost of visible tiles.
fn get_cost_matrix(world: &World) -> Vec<Vec<Option<i32>>> {
    let mut costs = vec![vec![None; world.width]; world.height];

    let positions = world.borrow_component_vec::<Position>().unwrap();
    let sprites = world.borrow_component_vec::<Sprite>().unwrap();
    let visibility = world.borrow_component_vec::<Visibility>().unwrap();
    let zip = positions
        .iter()
        .zip(sprites.iter())
        .zip(visibility.iter())
        .map(|((p, s), v): ((&Option<Position>, &Option<Sprite>), &Option<Visibility>)| (p, s, v));
    let world_info = zip.filter_map(
        |(p, c, v): (&Option<Position>, &Option<Sprite>, &Option<Visibility>)| {
            Some((p.as_ref()?, c.as_ref()?, v.as_ref()?))
        },
    );
    for (p, c, v) in world_info {
        if v.0 {
            costs[p.0.y][p.0.x] = Some(get_tile_cost(c.0));
        }
    }

    return costs;
}

// Explore a given point for the agent, and update the move state
pub fn explore(world: &mut World, agent: &mut AttackerAgent, p: Point) {
    let neighors = get_neighbors(p, world.width, world.height);
    let cost = agent.get_cost(p).unwrap();
    for n in neighors {
        let c = agent.get_cost(n);
        world.set_visible(n, true); // Set tile as visible
        if c.is_none() {
            let new_cost = cost + 1 + get_tile_cost(world.get_tile(n)); // Cost always increases by minimum of 1
            agent.update_cost(n, new_cost);
        }
    }
}

// Find a path between 2 arbitraty points
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
