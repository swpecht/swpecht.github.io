use running_emu::{run_sim, FeatureFlags};

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

    let features = FeatureFlags::new();
    let num_steps = run_sim(map, features);
    println!("Completed in {} steps", num_steps);
}
