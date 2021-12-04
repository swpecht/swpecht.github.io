use running_emu::{create_map, run_sim, FeatureFlags};

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

    // let map = &create_map(10);

    let mut features = FeatureFlags::new();
    features.write_agent_visible_map = true;
    let num_steps = run_sim(map, features);
    println!("Completed in {} steps", num_steps);
}
