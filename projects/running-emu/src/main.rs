use running_emu::{World, print_world, find_path_bfs, print_path};

fn main() {
    let map = 
    "S.........
    .WWWWWWWW.
    .....W....
    WWWW.W....
    .....W.WWW
    ..WWWW....
    WW...W....
    ..WWWW....
    .........G";

    let world = World::from_map(map);

    print_world(&world);
    println!("");
    let path = find_path_bfs(&world);
    print_path(&path, world);

}