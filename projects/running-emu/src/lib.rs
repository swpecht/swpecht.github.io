use std::cmp::max;

use crossterm::style::Color;
use hecs::World;

use crate::spatial::Point;

pub mod ai_pathing;
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

/// Populate a world from a string map
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
                    world.spawn((
                        Position(p),
                        Sprite(c),
                        Visibility(true),
                        Vision(1),
                        Agent,
                        TargetLocation(None),
                    ));
                    world.spawn((Position(p), Sprite('S'), Visibility(true)))
                }
                _ => world.spawn((Position(p), Sprite(c), Visibility(false))), // All others must be found
            };
        }
    }
}

/// Return Point where the goal is located
pub fn get_goal(world: &World) -> Point {
    for (_, (p, c)) in world.query::<(&Position, &Sprite)>().iter() {
        if c.0 == 'G' {
            return p.0;
        }
    }
    panic!("No goal found in world");
}

/// Return Point where the start is located
pub fn get_start(world: &World) -> Point {
    for (_, (p, c)) in world.query::<(&Position, &Sprite)>().iter() {
        if c.0 == 'S' {
            return p.0;
        }
    }
    panic!("No start found in world");
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
