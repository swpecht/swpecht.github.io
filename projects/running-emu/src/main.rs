use crossterm::{cursor::{MoveUp}, event::{Event, read}, execute, terminal::{Clear, ClearType}};
use itertools::izip;
use running_emu::{AttackerAgent, Position, Sprite, Visibility, World, attacker_system_update, get_path_from_agent, print_cost_matrix, print_path};
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

    let mut world = World::from_map(map);    
    let mut agent = AttackerAgent::new(&world);

    loop {
        if attacker_system_update(&mut world, &mut agent) {
            break;
        }
        render_system_update(&world);
        block_on_input(); // Only progress system updates on input
    }

    let path = get_path_from_agent(&world, &mut agent);
    print_path(&path, &world);
    println!("");
    print_cost_matrix(&world, &agent);

    // println!("Found in {} steps", steps);
}

/// Update the render of the player visible map
fn render_system_update(world: &World) {
    execute!(stdout(), Clear(ClearType::FromCursorDown)).unwrap();

    // Populate base layer
    let mut output = vec![vec!['?'; world.width]; world.height];

    // Draw over top with entities
    let zip = izip!(world.borrow_component_vec::<Position>().unwrap(), world.borrow_component_vec::<Sprite>().unwrap(), world.borrow_component_vec::<Visibility>().unwrap());
    let drawable = zip.filter_map(|(p, c, v): (&Option<Position>, &Option<Sprite>, &Option<Visibility>)| {Some((p.as_ref()?, c.as_ref()?, v.as_ref()?))});
    for (p, c, v) in drawable {
        if v.0 {
            output[p.0.y][p.0.x] = c.0;
        }        
    }

    for y in 0..world.height {
        output.push(Vec::new());
        for x in 0..world.width {
            print!("{}", output[y][x]);
        }
        println!("");
    }

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
