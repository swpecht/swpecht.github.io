use running_emu::{create_map, run_sim};

fn main() {
    let map = "....@..........
    ............WWW
    ...............
    ............WWW
    ...............
    ....WWW........
    .WWW.......WWW.
    .WGW.......W.W.
    ...............";

    // let map = ".@..
    // .WWW
    // .WGW
    // ....";

    // let map = &create_map(50);

    let num_steps = run_sim(map, true);
    println!("Completed in {} steps", num_steps);
}
