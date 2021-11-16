use crossterm::{cursor::{MoveUp}, event::{Event, read}, execute, terminal::{Clear, ClearType}};
use running_emu::{AttackerAgent, World, attacker_system_update, get_path_from_agent, print_agent_world, print_cost_matrix, print_path};
use std::io::stdout;

fn main() {
    let map = 
   "....S..........
    ............WWW
    ...............
    ............WWW
    ...............
    ....WWW........
    .WWW.......WWW.
    .WGW.......W.W.
    ...............";

    let world = World::from_map(map);
    let mut agent = AttackerAgent::new(&world);

    loop {
        if attacker_system_update(&world, &mut agent) {
            break;
        }
        render_system_update(&agent);
        block_on_input(); // Only progress system updates on input
    }

    println!("{}", world);
    let path = get_path_from_agent(&world, &mut agent);
    print_path(&path, &world);
    println!("");
    print_cost_matrix(&world, &agent);

    // println!("Found in {} steps", steps);
}



/// Update the render of the player visible map
fn render_system_update(agent: &AttackerAgent) {
    execute!(stdout(), Clear(ClearType::FromCursorDown)).unwrap();
    // execute!(stdout(), RestorePosition, Clear(ClearType::FromCursorDown)).unwrap();
    print_agent_world(&agent);
    execute!(stdout(), MoveUp(10)).unwrap();
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
