use running_emu::{create_map, run_sim_from_map, FeatureFlags};

fn main() {
    let map = "@..............
    ...............
    ...WWWDWWW.....
    ...WT.G.TW.....
    ...WWWWWWW.....
    ...............";

    let map = "@..............
    .WWWWWWWWWWWWW.
    .W...........W.
    .W.WWWWWWWWW.W.
    .W.W.......W.W.
    .W.WWWWWWW.W.W.
    .W......GW.W.W.
    .WWWWWWWWW.W.W.
    ...........W...";

    let map = &create_map(5);

    let mut features = FeatureFlags::new();
    features.write_agent_visible_map = false;
    features.print_tile_costs = true;
    let num_steps = run_sim_from_map(map, features);
    println!("Completed in {} steps", num_steps);
}
