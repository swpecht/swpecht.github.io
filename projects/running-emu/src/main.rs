use running_emu::{create_map, run_sim_from_map, FeatureFlags};

fn main() {
    let _map = "@...O..G
    ........";

    let _map = "@..............
    .OOOOOOOOOOOOO.
    .O...........O.
    .O.OOOOOOOOO.O.
    .O.O.......O.O.
    .O.OOOOOOO.O.O.
    .O......GO.O.O.
    .OOOOOOOOO.O.O.
    ...........O...";

    // let _map = &create_map(5);

    let mut features = FeatureFlags::new();
    features.write_agent_visible_map = false;
    // features.print_tile_costs = true;
    let num_steps = run_sim_from_map(_map, features);
    println!("Completed in {} steps", num_steps);
}
