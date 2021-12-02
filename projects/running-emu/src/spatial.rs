use hecs::{Entity, World};
use std::hash::Hash;

use crate::{get_max_point, Position, Sprite};

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
