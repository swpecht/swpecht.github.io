use running_emu::run_sim;

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

    let num_steps = run_sim(map, true);
    println!("Completed in {} steps", num_steps);
}
