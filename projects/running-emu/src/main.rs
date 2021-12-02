use crossterm::{
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
};
use hecs::World;

use running_emu::{
    get_max_point, print_cost_matrix, spatial::parse_map, system_ai, system_path_highlight,
    system_pathing, AttackerAgent, BackgroundHighlight, Position, Sprite, Visibility, Vision,
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
        system_vision(&mut world);
        if system_ai(&mut world, &mut agent) {
            break;
        }
        system_path_highlight(&mut world);
        system_pathing(&mut world);
        system_render(&mut world);
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
    let mut output_char = vec![vec!['?'; max_p.x]; max_p.y];
    let mut output_highlight = vec![vec![None; max_p.x]; max_p.y];

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

    for (_, (p, bg)) in world
        .query::<(&Position, &mut BackgroundHighlight)>()
        .iter()
    {
        output_highlight[p.0.y][p.0.x] = Some(bg.0);
        bg.0 = Color::Black; // Reset to black
    }

    for y in 0..max_p.y {
        output_char.push(Vec::new());
        for x in 0..max_p.x {
            let highlight = output_highlight[y][x];
            if let Some(color) = highlight {
                match execute!(stdout(), SetBackgroundColor(color)) {
                    Err(_) => panic!("error setting background color"),
                    _ => {}
                };
            }

            print!("{}", output_char[y][x]);
            match execute!(stdout(), ResetColor) {
                Err(_) => panic!("error reseting background color"),
                _ => {}
            };
        }
        println!("");
    }
    println!("");
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
