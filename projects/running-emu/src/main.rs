use crossterm::{
    event::{read, Event},
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
};
use running_emu::{
    attacker_system_update,
    ecs::BackgroundHighlight,
    ecs::Point,
    ecs::Sprite,
    ecs::Visibility,
    ecs::World,
    ecs::{ComponentType, Position},
    print_cost_matrix, AttackerAgent,
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

    let mut world = World::from_map(map);
    let mut agent = AttackerAgent::new(&world);
    let mut num_steps = 0;

    loop {
        num_steps += 1;
        render_system_update(&mut world);
        if attacker_system_update(&mut world, &mut agent) {
            break;
        }
        // block_on_input(); // Only progress system updates on input
    }

    println!("");
    print_cost_matrix(&world, &agent);
    println!("Completed in {} steps", num_steps);

    // println!("Found in {} steps", steps);
}

#[derive(Clone)]
struct RenderPoint {
    c: char,
    bg: Option<Color>,
}

/// Update the render of the player visible map
fn render_system_update(world: &mut World) {
    // execute!(stdout(), Clear(ClearType::FromCursorDown)).unwrap();

    // Populate base layer
    let mut output = vec![vec![RenderPoint { c: '?', bg: None }; world.width]; world.height];

    {
        let it = world.filter(vec![
            ComponentType::Position,
            ComponentType::Visibility,
            ComponentType::Sprite,
        ]);
        for a in it.archetypes {
            let pos = a.borrow_component_vec::<Position>().unwrap();
            let vis = a.borrow_component_vec::<Visibility>().unwrap();
            let spr = a.borrow_component_vec::<Sprite>().unwrap();

            for i in 0..a.length {
                // TODO: remove after fully implemented archetypes
                match (pos[i].as_ref(), vis[i].as_ref(), spr[i].as_ref()) {
                    (Some(p), Some(v), Some(s)) => {
                        if v.0 {
                            // Handle special case for '.' only draw if nothing else present
                            if s.0 == '.' && output[p.0.y][p.0.x].c != '?' {
                                // Do nothing, '.' can be in background
                            } else {
                                output[p.0.y][p.0.x].c = s.0;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let height = world.height;
    let width = world.width;
    let it = world.filter(vec![ComponentType::BackgroundHighlight]);
    for a in it.archetypes {
        // TODO: remove after fully implemented archetypes
        let mut hig = match a.borrow_mut_component_vec::<BackgroundHighlight>() {
            Some(a) => a,
            _ => continue, // no highlights to populate
        };
        let pos = a.borrow_component_vec::<Position>().unwrap();

        for i in 0..a.length {
            // TODO: remove after fully implemented archetypes
            match (pos[i].as_ref(), hig[i].as_ref()) {
                (Some(p), Some(bg)) => {
                    output[p.0.y][p.0.x].bg = Some(bg.0);
                    hig[i] = None;
                }
                _ => {}
            }
        }
    }

    for y in 0..height {
        for x in 0..width {
            let color = output[y][x].bg;
            if color.is_some() {
                match execute!(stdout(), SetBackgroundColor(color.unwrap())) {
                    Err(_) => panic!("error setting background color"),
                    _ => {}
                };
            }
            print!("{}", output[y][x].c);
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

/// Blocks until user input
fn block_on_input() {
    loop {
        // `read()` blocks until an `Event` is available
        match read() {
            Ok(event) => match event {
                Event::Key(_event) => break,
                _ => {}
            },
            _ => {
                panic!("Error reading input")
            }
        }
    }
}
