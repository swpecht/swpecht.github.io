use std::{
    cmp::max,
    fs::File,
    io::{stdout, Write},
};

use crossterm::{
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
};
use hecs::World;
use spatial::SpatialCache;

use crate::{
    ai_pathing::{system_ai, system_path_highlight, system_pathing},
    spatial::Point,
};

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

#[derive(Clone, Copy)]
pub struct FeatureFlags {
    /// Enable rendering to stdout
    pub render: bool,
    /// Enable the cache for `get_entity(Point)` calls
    pub entity_spatial_cache: bool,
    /// When calculating the `goal` score for the exploration AI, use a travel matrix
    /// rather than calling `get_path` for each call
    pub travel_matrix_for_goal_distance: bool,
    /// Write the agent visible map to `output.txt`
    pub write_agent_visible_map: bool,
}

impl FeatureFlags {
    pub fn new() -> Self {
        return Self {
            render: true,
            entity_spatial_cache: true,
            travel_matrix_for_goal_distance: true,
            write_agent_visible_map: false,
        };
    }
}

/// Returns the cost to reach the goal.
///
/// Main entry point for running a simulation
pub fn run_sim(map: &str, features: FeatureFlags) -> i32 {
    let mut world = hecs::World::new();
    parse_map(&mut world, map);

    let mut num_steps = 0;

    let mut output_file = File::create("output.txt").unwrap();

    loop {
        num_steps += 1;
        let spatial_cache = match features.entity_spatial_cache {
            true => Some(SpatialCache::new(&world)),
            false => None,
        };

        system_vision(&mut world);
        let char_buffer = build_char_output(&world);
        let highlight_buffer = build_highlight_output(&mut world);

        if features.write_agent_visible_map {
            write_state(&char_buffer, &mut output_file).expect("error writing state");
        }

        if features.render {
            system_render(&char_buffer, &highlight_buffer);
        }
        if system_ai(&mut world, features) {
            break;
        }
        system_path_highlight(&mut world, spatial_cache.as_ref());
        system_pathing(&mut world);
    }
    // print_travel_cost_matrix(&world);
    return num_steps;
}

/// Populate a world from a string map
fn parse_map(world: &mut World, map: &str) {
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

/// Build the grid of character outputs
fn build_char_output(world: &World) -> Vec<Vec<char>> {
    let max_p = get_max_point(world);
    // Populate base layer
    let mut output_char = vec![vec!['?'; max_p.x]; max_p.y];
    // Draw over top with entities
    for (_, (p, c, v)) in world.query::<(&Position, &Sprite, &Visibility)>().iter() {
        if v.0 {
            // Handle special case for '.' only draw if nothing else present
            if c.0 == '.' && output_char[p.0.y][p.0.x] != '?' {
                // Do nothing, '.' can be in background
            } else {
                output_char[p.0.y][p.0.x] = c.0;
            }
        }
    }

    return output_char;
}

fn write_state(chars: &Vec<Vec<char>>, f: &mut File) -> std::io::Result<()> {
    for y in 0..chars.len() {
        for x in 0..chars[0].len() {
            write!(f, "{}", chars[y][x])?;
        }
        writeln!(f, "")?;
    }
    writeln!(f, "")
}

fn build_highlight_output(world: &mut World) -> Vec<Vec<Option<Color>>> {
    let max_p = get_max_point(world);
    let mut output_highlight = vec![vec![None; max_p.x]; max_p.y];

    for (_, (p, bg)) in world
        .query::<(&Position, &mut BackgroundHighlight)>()
        .iter()
    {
        output_highlight[p.0.y][p.0.x] = Some(bg.0);
        bg.0 = Color::Black; // Reset to black
    }

    return output_highlight;
}

/// Update the render of the player visible map
pub fn system_render(output_char: &Vec<Vec<char>>, output_highlight: &Vec<Vec<Option<Color>>>) {
    for y in 0..output_char.len() {
        for x in 0..output_char[0].len() {
            let highlight = output_highlight[y][x];
            if let Some(color) = highlight {
                execute!(stdout(), SetBackgroundColor(color)).unwrap();
                print!("{}", output_char[y][x]);
                execute!(stdout(), ResetColor).unwrap();
            } else {
                print!("{}", output_char[y][x]);
            }
        }
        println!("");
    }
    println!("");
}

pub fn system_vision(world: &mut World) {
    let mut ids = Vec::new();
    for (id, (_, _)) in world.query_mut::<(&Position, &Vision)>() {
        ids.push(id);
    }

    for id in ids {
        let agent_pos = world.get::<Position>(id).unwrap().0;
        let agent_sight = world.get::<Vision>(id).unwrap().0;
        for (_, (position, visibility)) in world.query_mut::<(&Position, &mut Visibility)>() {
            if agent_pos.dist(&position.0) <= agent_sight as i32 {
                visibility.0 = true;
            }
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
                (0, 0) => '@',
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
    fn create_map_empty() {
        let map = create_map(3);
        assert_eq!(map, "@..\n...\n..G")
    }

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

    #[test]
    fn test_spiral_map() {
        let map = "@..............
                        .WWWWWWWWWWWWW.
                        .W...........W.
                        .W.WWWWWWWWW.W.
                        .W.W.......W.W.
                        .W.WWWWWWW.W.W.
                        .W......GW.W.W.
                        .WWWWWWWWW.W.W.
                        ...........W...";

        let mut features = FeatureFlags::new();
        features.render = false;
        features.write_agent_visible_map = true;
        let num_steps = run_sim(map, features);
        assert_eq!(num_steps, 189)
    }
}
