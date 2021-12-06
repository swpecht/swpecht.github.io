use running_emu::{run_sim, FeatureFlags};

fn main() {
    let map = "@..............
    ...............
    ...............
    .....WWWWW.....
    .....W.G.W.....
    .....WWWWW.....
    ...............";

    // let map = ".@..
    // .WWW
    // .WGW
    // ....";

    // let map = &create_map(10);

    let mut features = FeatureFlags::new();
    features.write_agent_visible_map = true;
    let num_steps = run_sim(map, features);
    println!("Completed in {} steps", num_steps);
}
