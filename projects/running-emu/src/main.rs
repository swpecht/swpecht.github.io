use crossterm::{
    execute,
    style::{ResetColor, SetBackgroundColor},
};
use hecs::World;
use running_emu::{
    attacker_system_update, get_max_point, print_cost_matrix,
    spatial::get_entity,
    spatial::{parse_map, Point},
    AttackerAgent, BackgroundHighlight, Position, Sprite, Velocity, Visibility, Vision,
};
use std::io::stdout;

fn main() {
    let map = "@..............
    .WWWWWWWWWWWWW.
    .W...........W.
    .W.WWWWWWWWW.W.
    .W.W.......W.W.
    .W.WWWWWWW.W.W.
    .W......GW.W.W.
    .WWWWWWWWW.W.W.
    ...........W...";

    // let map = ".@..
    // .WWW
    // .WGW
    // ....";

    let mut world = hecs::World::new();
    parse_map(&mut world, map);

    let mut agent = AttackerAgent::new(&world);
    let mut num_steps = 0;

    loop {
        num_steps += 1;
        system_render(&mut world);
        system_vision(&mut world);
        if attacker_system_update(&mut world, &mut agent) {
            break;
        }
        system_velocity(&mut world);
        // block_on_input(); // Only progress system updates on input
    }

    println!("");
    print_cost_matrix(&world, &agent);
    println!("Completed in {} steps", num_steps);

    // println!("Found in {} steps", steps);
}

/// Update the render of the player visible map
fn system_render(world: &mut World) {
    let max_p = get_max_point(world);

    // Populate base layer
    let mut output = vec![vec!['?'; max_p.x]; max_p.y];

    // Draw over top with entities
    for (_, (p, c, v)) in world.query::<(&Position, &Sprite, &Visibility)>().iter() {
        if v.0 {
            // Handle special case for '.' only draw if nothing else present
            if c.0 == '.' && output[p.0.y][p.0.x] != '?' {
                // Do nothing, '.' can be in background
            } else {
                output[p.0.y][p.0.x] = c.0;
            }
        }
    }

    for y in 0..max_p.y {
        output.push(Vec::new());
        for x in 0..max_p.x {
            let id = get_entity(world, Point { x: x, y: y });
            let highlight = world.get::<BackgroundHighlight>(id.unwrap());
            if id.is_some() && highlight.is_ok() {
                let color = highlight.unwrap().0;
                match execute!(stdout(), SetBackgroundColor(color)) {
                    Err(_) => panic!("error setting background color"),
                    _ => {}
                };
                world
                    .remove_one::<BackgroundHighlight>(id.unwrap())
                    .unwrap();
            }

            print!("{}", output[y][x]);
            match execute!(stdout(), ResetColor) {
                Err(_) => panic!("error reseting background color"),
                _ => {}
            };
        }
        println!("");
    }
    println!("");

    // execute!(stdout(), MoveUp(10)).unwrap();
}

fn system_vision(world: &mut World) {
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

fn system_velocity(world: &mut World) {
    for (_, (pos, vel)) in world.query_mut::<(&mut Position, &mut Velocity)>() {
        pos.0 = Point {
            x: (pos.0.x as i32 + vel.0) as usize,
            y: (pos.0.y as i32 + vel.1) as usize,
        };

        // Set velocity to 0 after consumed
        vel.0 = 0;
        vel.1 = 0;
    }
}
