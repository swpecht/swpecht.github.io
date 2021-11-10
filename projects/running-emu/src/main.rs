use running_emu::{World, print_world, find_path_bfs, print_path};

fn main() {
    let world = World::new();

    print_world(&world);
    println!("");
    let path = find_path_bfs(&world);
    print_path(&path, world);

}