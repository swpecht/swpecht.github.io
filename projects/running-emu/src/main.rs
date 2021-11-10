struct World {
    costs: [i32; 100],
    start: Point,
    goal: Point,
}

#[derive(PartialEq)]
struct Point {
    x: usize,
    y: usize
}

fn new_world() -> World {
    World { costs: [0; 100], start: Point {x: 0, y: 0}, goal: Point{x: 9, y: 9} }
}

fn main() {
    let world = new_world();

    print_world(world);
}

fn print_world(world: World) {
    for y in 0..10 {
        for x in 0..10 {
            let p = Point{x: x, y: y};

            if p == world.start {
                print!("O")
            } else if p == world.goal {
                print!("X")
            } else {
                print!("{}", world.costs[y * 10 + x])
            }            
        }
        println!("");
    }
}
