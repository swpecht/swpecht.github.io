use running_emu::{create_map, run_sim_from_map, FeatureFlags};

fn main() {
    let _map = "@..............
    ...............
    ...WWWDWWW.....
    ...WT.G.TW.....
    ...WWWWWWW.....
    ...............";

    let _map = "@..............
    .WWWWWWWWWWWWW.
    .W...........W.
    .W.WWWWWWWWW.W.
    .W.W.......W.W.
    .W.WWWWWWW.W.W.
    .W......GW.W.W.
    .WWWWWWWWW.W.W.
    ...........W...";

    // let _map = &create_map(5);

    let mut features = FeatureFlags::new();
    features.write_agent_visible_map = false;
    features.print_tile_costs = true;
    let num_steps = run_sim_from_map(_map, features);
    println!("Completed in {} steps", num_steps);
}
