use hecs::{Entity, World};
use std::{hash::Hash, vec};

use crate::{get_max_point, Agent, Position, Sprite, Visibility, Vision};

pub fn parse_map(world: &mut World, map: &str) {
    let mut x = 0;
    let mut y = 0;
    let mut width = None;
    let mut tiles = vec![vec![]];

    // calculate width
    for c in map.chars() {
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

    for y in 0..tiles.len() {
        for x in 0..tiles[0].len() {
            let c = tiles[y][x];
            let p = Point { x: x, y: y };
            let _ = match c {
                'G' => world.spawn((Position(p), Sprite(c), Visibility(true))), // Goal and Start are visible to begin
                '@' => {
                    // Also spawn a visible start position
                    world.spawn((Position(p), Sprite(c), Visibility(true), Vision(1), Agent));
                    world.spawn((Position(p), Sprite('S'), Visibility(true)))
                }
                _ => world.spawn((Position(p), Sprite(c), Visibility(false))), // All others must be found
            };
        }
    }
}

/// Returns tile at a given location or '.' if no entities present
pub fn get_tile(world: &World, p: Point) -> char {
    let id = get_entity(world, p);

    match id {
        Some(id) => world.get::<Sprite>(id).unwrap().0,
        _ => '.',
    }
}

pub fn get_entity(world: &World, p: Point) -> Option<Entity> {
    let mut positions = world.query::<&Position>();
    for (id, candidate) in positions.iter() {
        match p {
            _ if candidate.0 == p => return Some(id),
            _ => {}
        }
    }
    return None;
}

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
    let max_p = get_max_point(world);
    for y in 0..max_p.x {
        for x in 0..max_p.y {
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
        parse_map(&mut world, map);
        let max_p = get_max_point(&world);
        assert_eq!(max_p.x, 3);
        assert_eq!(max_p.y, 4);
    }
}
