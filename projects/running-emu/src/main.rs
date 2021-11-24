use crossterm::{
    event::{read, Event},
    execute,
    style::{ResetColor, SetBackgroundColor},
};
use running_emu::{
    attacker_system_update, map::BackgroundHighlight, map::Map, map::Point, map::Position,
    map::Sprite, map::Visibility, print_cost_matrix, AttackerAgent,
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

    let mut map = Map::new(map, &mut world);
    let mut agent = AttackerAgent::new(&map);
    let mut num_steps = 0;

    loop {
        num_steps += 1;
        render_system_update(&mut map);
        if attacker_system_update(&mut map, &mut agent) {
            break;
        }
        // block_on_input(); // Only progress system updates on input
    }

    println!("");
    print_cost_matrix(&map, &agent);
    println!("Completed in {} steps", num_steps);

    // println!("Found in {} steps", steps);
}

/// Update the render of the player visible map
fn render_system_update(map: &mut Map) {
    // execute!(stdout(), Clear(ClearType::FromCursorDown)).unwrap();

    // Populate base layer
    let mut output = vec![vec!['?'; map.width]; map.height];

    // Draw over top with entities
    for (_, (p, c, v)) in map
        .world
        .query::<(&Position, &Sprite, &Visibility)>()
        .iter()
    {
        if v.0 {
            // Handle special case for '.' only draw if nothing else present
            if c.0 == '.' && output[p.0.y][p.0.x] != '?' {
                // Do nothing, '.' can be in background
            } else {
                output[p.0.y][p.0.x] = c.0;
            }
        }
    }

    for y in 0..map.height {
        output.push(Vec::new());
        for x in 0..map.width {
            let id = map.get_entity(Point { x: x, y: y });
            let highlight = map.world.get::<BackgroundHighlight>(id.unwrap());
            if id.is_some() && highlight.is_ok() {
                let color = highlight.unwrap().0;
                match execute!(stdout(), SetBackgroundColor(color)) {
                    Err(_) => panic!("error setting background color"),
                    _ => {}
                };
                map.world.remove_one::<BackgroundHighlight>(id.unwrap());
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
