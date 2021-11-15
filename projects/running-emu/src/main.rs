use running_emu::{AttackerAgent, World, find_path_bfs, print_cost_matrix, print_path};

fn main() {
    let map = 
    "....S.....
    ..........
    ..........
    ..........
    ..........
    ....WWW...
    ....WGWWWW
    ....W.W...
    ..........";

    let world = World::from_map(map);
    let mut agent = AttackerAgent::new(&world);

    println!("{}", world);
    let path = find_path_bfs(&world, &mut agent);
    print_path(&path, &world);
    println!("");

    print_cost_matrix(&world, &agent);

}