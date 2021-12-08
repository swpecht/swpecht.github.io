use core::time;
use std::{
    thread,
    time::{Duration, Instant},
};

use hecs::World;
use log::info;
use running_emu::{
    ai_pathing::{get_goal_lpapather, get_start_lpapather},
    create_map, parse_map,
    render::system_render,
    run_sim_from_map, step_game_world, system_vision, FeatureFlags,
};
use sdl2::{event::Event, keyboard::Keycode, pixels, render::Canvas, video::Window, EventPump};

const FRAME_TIME_MILLI: Duration = time::Duration::from_millis(500);
fn main() -> Result<(), String> {
    let (mut canvas, mut events) = init_sdl()?;

    // let _map = "@...O..G
    // ........";

    let _map = "@..............
    .OOOOOOOOOOOOO.
    .O...........O.
    .O.OOOOOOOOO.O.
    .O.O.......O.O.
    .O.OOOOOOO.O.O.
    .O......GO.O.O.
    .OOOOOOOOO.O.O.
    ...........O...";

    let _map = &create_map(25);

    let mut features = FeatureFlags::new();
    features.write_agent_visible_map = false;
    // features.print_tile_costs = true;
    // let num_steps = run_sim_from_map(_map, features);
    // println!("Completed in {} steps", num_steps);
    let mut world = World::new();
    parse_map(&mut world, _map);

    let mut start_pather = get_start_lpapather(&world);
    let mut goal_pather = get_goal_lpapather(&world);

    // Bootstrap
    system_vision(&mut world);

    loop {
        let start = Instant::now();

        canvas.set_draw_color(pixels::Color::RGB(0, 0, 0));
        canvas.clear();
        if system_input(&mut events) {
            break;
        };

        step_game_world(&mut world, features, &mut start_pather, &mut goal_pather);
        system_render(&world, &mut canvas)?;

        canvas.present();

        info!("frame completed in: {} ms", start.elapsed().as_millis());
        while start.elapsed() < FRAME_TIME_MILLI {
            thread::sleep(FRAME_TIME_MILLI - start.elapsed())
        }
    }

    Ok(())
}

fn init_sdl() -> Result<(Canvas<Window>, EventPump), String> {
    let sdl_context = sdl2::init()?;
    let video_subsys = sdl_context.video()?;
    let window = running_emu::render::get_window(video_subsys)?;

    let canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    let events = sdl_context.event_pump()?;

    Ok((canvas, events))
}

fn system_input(events: &mut EventPump) -> bool {
    for event in events.poll_iter() {
        match event {
            Event::Quit { .. } => return true,

            Event::KeyDown {
                keycode: Some(keycode),
                ..
            } => {
                if keycode == Keycode::Escape {
                    return true;
                }
            }

            _ => {}
        }
    }
    return false;
}
