use crossterm::{cursor::{MoveUp}, event::{Event, read}, execute, style::{Color, ResetColor, SetBackgroundColor}, terminal::{Clear, ClearType}};
use itertools::izip;
use running_emu::{AttackerAgent, BackgroundHighlight, Point, Position, Sprite, Visibility, World, attacker_system_update, print_cost_matrix};
use std::io::stdout;

fn main() {
//     let map = 
//    "....S@.........
//     ............WWW
//     ...............
//     ............WWW
//     ...............
//     ....WWW........
//     .WWW.......WWW.
//     .WGW.......W.W.
//     ...............";

    let map = 
   "S@..
    .WWW
    .WGW
    ....";

    let mut world = World::from_map(map);    
    let mut agent = AttackerAgent::new(&world);

    loop {
        render_system_update(&world);
        if attacker_system_update(&mut world, &mut agent) {
            break;
        }
        // block_on_input(); // Only progress system updates on input
    }

    println!("");
    print_cost_matrix(&world, &agent);

    // println!("Found in {} steps", steps);
}

/// Update the render of the player visible map
fn render_system_update(world: &World) {
    // execute!(stdout(), Clear(ClearType::FromCursorDown)).unwrap();

    // Populate base layer
    let mut output = vec![vec!['?'; world.width]; world.height];

    // Draw over top with entities
    let positions = world.borrow_component_vec::<Position>().unwrap();
    let sprites = world.borrow_component_vec::<Sprite>().unwrap();
    let visibility = world.borrow_component_vec::<Visibility>().unwrap();
    let zip = positions.iter().zip(sprites.iter()).zip(visibility.iter()).map(|((p, s), v): ((&Option<Position>, &Option<Sprite>), &Option<Visibility>)| {(p, s, v)});
    let drawable = zip.filter_map(|(p, c, v): (&Option<Position>, &Option<Sprite>, &Option<Visibility>)| {Some((p.as_ref()?, c.as_ref()?, v.as_ref()?))});
    for (p, c, v) in drawable {
        if v.0 {
            // Handle special case for '.' only draw if nothing else present
            if c.0 == '.' && output[p.0.y][p.0.x] != '?' {
                // Do nothing, '.' can be in background
            } else {
                output[p.0.y][p.0.x] = c.0;
            }
        }        
    }

    let mut highlights = world.borrow_mut_component_vec::<BackgroundHighlight>();
    for y in 0..world.height {
        output.push(Vec::new());
        for x in 0..world.width {
            let id = world.get_entity(Point{x: x, y: y});
            if id.is_some() && highlights.as_ref().is_some() && highlights.as_ref().unwrap()[id.unwrap()].as_ref().is_some() {
                let color = highlights.as_ref().unwrap()[id.unwrap()].as_ref().unwrap().0;
                execute!(stdout(), SetBackgroundColor(color));
                highlights.as_mut().unwrap()[id.unwrap()] = Some(BackgroundHighlight(Color::Black));                
            }

            print!("{}", output[y][x]);
            execute!(stdout(), ResetColor);
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
            _ => {panic!("Error reading input")}
        }
    }
}
