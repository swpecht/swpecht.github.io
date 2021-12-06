use std::{
    cmp::max,
    fs::File,
    io::{stdout, Write},
};

use ai::system_defense_ai;
use ai_pathing::{get_goal_lpapather, get_start_lpapather, system_print_tile_costs};
use crossterm::{
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
};
use hecs::{Entity, World};
use spatial::system_update_spatial_cache;

use crate::{
    ai_pathing::{system_ai_action, system_exploration, system_path_highlight},
    spatial::Point,
};

pub mod ai;
pub mod ai_pathing;
pub mod spatial;

/// Position of the entity in the game world
pub struct Position(pub Point);
pub struct Sprite(pub char);
pub struct Visibility(pub bool);
/// Determine if the background for an entity should be highlighted
pub struct BackgroundHighlight(pub Color);
/// How far an entity can see.
pub struct Vision(pub usize);
pub struct TargetLocation(pub Option<Point>);
pub struct AttackerAgent;
pub struct Health(pub i32);
pub struct Damage {
    pub amount: i32,
    pub from: Entity,
}
/// Damage a unit can do
pub struct Attack {
    pub damage: i32,
    pub range: usize,
}

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
    pub pathing_algorithm: PathingAlgorithm,
    pub print_tile_costs: bool,
}

#[derive(Clone, Copy)]
pub enum PathingAlgorithm {
    Astar,
    LpaStar,
}

impl FeatureFlags {
    pub fn new() -> Self {
        return Self {
            render: true,
            entity_spatial_cache: true,
            travel_matrix_for_goal_distance: true,
            write_agent_visible_map: false,
            pathing_algorithm: PathingAlgorithm::LpaStar,
            print_tile_costs: false,
        };
    }
}

/// Returns the cost to reach the goal.
///
/// Main entry point for running a simulation
pub fn run_sim_from_map(map: &str, features: FeatureFlags) -> i32 {
    let mut world = hecs::World::new();
    parse_map(&mut world, map);
    return run_sim(&mut world, features);
}

fn run_sim(world: &mut World, features: FeatureFlags) -> i32 {
    let mut num_steps = 0;

    let mut output_file = File::create("output.txt").unwrap();
    let mut start_pather = get_start_lpapather(&world);
    let mut goal_pather = get_goal_lpapather(&world);

    let max_p = get_max_point(&world);
    let mut char_buffer = vec![vec!['?'; max_p.x]; max_p.y];

    // Bootstrap
    system_vision(world);

    loop {
        num_steps += 1;

        if features.entity_spatial_cache {
            system_update_spatial_cache(world);
        }

        if system_exploration(world, features, &mut start_pather, &mut goal_pather) {
            break;
        }

        system_path_highlight(world);
        build_char_output(&world, &mut char_buffer);
        let highlight_buffer = build_highlight_output(world);
        if features.write_agent_visible_map {
            write_state(&char_buffer, &mut output_file).expect("error writing state");
        }
        if features.render {
            system_render(&char_buffer, &highlight_buffer);
        }

        if features.print_tile_costs {
            system_print_tile_costs(world);
        }

        system_ai_action(world);
        system_defense_ai(world);
        system_vision(world);
        system_health(world); // Can despawn enemies so, should be run last
    }
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
            _ => {
                tiles[y].push(c);
                x += 1
            }
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
                        AttackerAgent,
                        TargetLocation(None),
                        Attack {
                            damage: 1,
                            range: 1,
                        },
                        Health(500),
                    ));
                    world.spawn((Position(p), Sprite('S'), Visibility(true)))
                }
                'W' => {
                    world.spawn((Position(p), Sprite(c), Visibility(false), Health(50)));
                    world.spawn((Position(p), Sprite('.'), Visibility(false))) // Spawn empty tile underneath
                }
                'D' => {
                    world.spawn((Position(p), Sprite(c), Visibility(false), Health(25)));
                    world.spawn((Position(p), Sprite('.'), Visibility(false))) // Spawn empty tile underneath
                }
                'T' => {
                    world.spawn((
                        Position(p),
                        Sprite(c),
                        Visibility(false),
                        Health(5),
                        Attack {
                            range: 2,
                            damage: 5,
                        },
                    ));
                    world.spawn((Position(p), Sprite('.'), Visibility(false))) // Spawn empty tile underneath
                }
                '.' => world.spawn((Position(p), Sprite(c), Visibility(false))), // All others must be found
                _ => panic!("Error spawning entities, unknown tile: {}", c),
            };
        }
    }
}

/// Build the grid of character outputs
fn build_char_output(world: &World, buffer: &mut Vec<Vec<char>>) {
    // Reset the buffer
    for y in 0..buffer.len() {
        for x in 0..buffer[0].len() {
            buffer[y][x] = '?';
        }
    }

    for (_, (p, c, v)) in world.query::<(&Position, &Sprite, &Visibility)>().iter() {
        if v.0 {
            // Handle special case for '.' only draw if nothing else present
            if c.0 == '.' && buffer[p.0.y][p.0.x] != '?' {
                // Do nothing, '.' can be in background
            } else {
                buffer[p.0.y][p.0.x] = c.0;
            }
        }
    }
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
    // Make entities visible based on line of sight
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

    // Make entities visible based on attacking
    let mut attackers = Vec::new();
    for (_, dmg) in world.query_mut::<&Damage>() {
        attackers.push(dmg.from);
    }

    for attacker in attackers {
        world.insert_one(attacker, Visibility(true)).unwrap();
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

pub fn system_health(world: &mut World) {
    let mut damage_removal = Vec::new();
    let mut entity_despawn = Vec::new();

    for (e, (health, dmg)) in world.query_mut::<(&mut Health, &Damage)>() {
        health.0 = health.0 - dmg.amount;

        if health.0 <= 0 {
            entity_despawn.push(e);
        } else {
            damage_removal.push(e);
        }
    }

    for e in damage_removal {
        world.remove_one::<Damage>(e).unwrap();
    }

    for e in entity_despawn {
        world.despawn(e).unwrap();
    }
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
        let num_steps = run_sim_from_map(map, features);
        assert_eq!(num_steps, 375)
    }

    #[test]
    fn test_empty_map() {
        let map = &&create_map(10);

        let mut features = FeatureFlags::new();
        features.render = true;
        let num_steps = run_sim_from_map(map, features);
        assert_eq!(num_steps, 19)
    }

    #[test]
    fn test_health_system() {
        let mut world = hecs::World::new();
        let attacker = world.spawn(());

        let e = world.spawn((
            Health(10),
            Damage {
                amount: 5,
                from: attacker,
            },
        ));
        assert_eq!(world.len(), 2);
        system_health(&mut world);
        assert_eq!(world.get::<Health>(e).unwrap().0, 5);
        system_health(&mut world);
        // No more damage done, it was consumed
        assert_eq!(world.get::<Health>(e).unwrap().0, 5);
        world
            .insert_one(
                e,
                Damage {
                    amount: 10,
                    from: attacker,
                },
            )
            .unwrap();

        system_health(&mut world);
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn test_attacking() {
        // Map of:
        // @WG
        let mut world = hecs::World::new();
        world.spawn((
            Sprite('S'),
            Position(Point { x: 0, y: 0 }),
            Visibility(true),
        ));
        world.spawn((
            Position(Point { x: 0, y: 0 }),
            Visibility(true),
            AttackerAgent,
            TargetLocation(None),
            Attack {
                damage: 1,
                range: 1,
            },
        ));
        world.spawn((Health(10), Position(Point { x: 1, y: 0 }), Visibility(true)));
        world.spawn((Position(Point { x: 1, y: 0 }), Visibility(true)));
        world.spawn((
            Sprite('G'),
            Position(Point { x: 2, y: 0 }),
            Visibility(true),
        ));

        let mut features = FeatureFlags::new();
        features.render = false;
        features.write_agent_visible_map = true;
        let num_steps = run_sim(&mut world, features);
        assert_eq!(num_steps, 13)
    }
}
