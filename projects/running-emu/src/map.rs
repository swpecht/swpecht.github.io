use crossterm::style::Color;
use hecs::{Entity, World};
use std::{hash::Hash, vec};

/// Represents the game world
pub struct Map<'a> {
    pub world: &'a mut World,
    pub width: usize,
    pub height: usize,
}

impl<'a> Map<'a> {
    /// Create a world from a string representation of a map.
    ///
    /// Maps are a series of characters with \\n representing new lines.
    /// All of the rows on a map must have the same number of characters.
    /// As an example, a simple map would be:
    /// S..
    /// .W.
    /// ..G
    /// Where, for example, S represents the start, W a wall, and G the goal.
    pub fn new(str_map: &str, world: &'a mut World) -> Map<'a> {
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
            world: world,
            width: width.unwrap(),
            height: y + 1,
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
        let _ = match c {
            'G' | '@' => self.world.spawn((Position(p), Sprite(c), Visibility(true))), // Goal and Start are visible to begin
            _ => self
                .world
                .spawn((Position(p), Sprite(c), Visibility(false))), // All others must be found
        };

        if c == '@' {
            // Create a Start entity where agent started
            self.world
                .spawn((Position(p), Sprite('S'), Visibility(true)));
        }
    }

    /// Returns tile at a given location or '.' if no entities present
    pub fn get_tile(&mut self, p: Point) -> char {
        let id = self.get_entity(p);

        match id {
            Some(id) => self.world.get::<Sprite>(id).unwrap().0,
            _ => '.',
        }
    }

    pub fn set_visible(&mut self, p: Point, vis: bool) {
        let id = self.get_entity(p);

        match id {
            Some(id) => {
                self.world.insert_one(id, Visibility(vis)).unwrap();
            }
            _ => {}
        }
    }

    /// Return the entity at a given point if one exists
    pub fn get_entity(&self, p: Point) -> Option<Entity> {
        let mut positions = self.world.query::<&Position>();
        for (id, candidate) in positions.iter() {
            match p {
                _ if candidate.0 == p => return Some(id),
                _ => {}
            }
        }
        return None;
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

pub fn print_path(path: &Vec<Point>, world: &Map) {
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
    #[allow(unused_imports)]
    use super::*;
    #[allow(unused_imports)]
    use hecs::World;

    #[test]
    fn test_simple_map_parse() {
        let map = "@..
        ...
        ...
        ..G";

        let mut world = World::new();
        let map = Map::new(map, &mut world);
        assert_eq!(map.width, 3);
        assert_eq!(map.height, 4);
    }
}
