use crossterm::{cursor::{MoveUp}, event::{Event, read}, execute, terminal::{Clear, ClearType}};
use itertools::izip;
use running_emu::{AttackerAgent, Point, World, attacker_system_update, get_path_from_agent, print_cost_matrix, print_path};
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
    let zip = izip!(&world.position_components, &world.sprite_components, &world.vis_components);
    let drawable = zip.filter_map(|(p, c, v): (&Option<Point>, &Option<char>, &Option<bool>)| {Some((p.as_ref()?, c.as_ref()?, v.as_ref()?))});
    for (p, c, v) in drawable {
        if *v {
            output[p.y][p.x] = *c;
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
