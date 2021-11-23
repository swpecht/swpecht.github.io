use crossterm::style::Color;
use std::{
    cell::{Ref, RefCell, RefMut},
    hash::Hash,
    vec,
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
        return Self {
            entities_count: 0,
            width: 0,
            height: 0,
            component_vecs: Vec::new(),
        };
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
                '.' | 'W' | 'G' | '@' => {
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
            'G' | '@' => self.add_component_to_entity(id, Visibility(true)), // Goal and Start are visible to begin
            _ => {} // All others must be found
        }

        if c == '@' {
            // Create a Start entity where agent started
            let bg = self.new_entity();
            self.add_component_to_entity(bg, Position(p));
            self.add_component_to_entity(bg, Sprite('S'));
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

mod test {
    use super::*;

    #[test]
    fn test_add_entity() {
        let mut world = World::new();
        world.new_entity();
        world.new_entity();
        world.new_entity();

        assert_eq!(world.entities_count, 3);
    }

    #[test]
    fn test_add_components() {
        let mut world = World::new();

        let a = world.new_entity();
        world.add_component_to_entity(a, Sprite('X'));
        world.add_component_to_entity(a, Visibility(true));

        let b = world.new_entity();
        world.add_component_to_entity(b, Sprite('X'));
        world.add_component_to_entity(b, Visibility(false));

        let c = world.new_entity();
        world.add_component_to_entity(c, Sprite('X'));

        let sprites = world.borrow_component_vec::<Sprite>().unwrap();
        let num_sprites = sprites
            .iter()
            .filter_map(|a: &Option<Sprite>| Some(a.as_ref()?))
            .count();
        assert_eq!(num_sprites, 3);

        let vis = world.borrow_component_vec::<Visibility>().unwrap();
        let num_vis = vis
            .iter()
            .filter_map(|a: &Option<Visibility>| Some(a.as_ref()?))
            .count();
        assert_eq!(num_vis, 2);
    }
}
