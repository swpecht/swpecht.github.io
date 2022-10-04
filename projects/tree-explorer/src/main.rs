extern crate sdl2;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::{Point, Rect};

#[derive(Debug)]
enum Input {
    ZoomOut,
    ZoomIn,
}

struct Node {
    ctr: Point,
    name: String,
}

const SIZE: i32 = 10;

fn main() -> Result<(), String> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("Tree-viewer", 800, 600)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window
        .into_canvas()
        .software()
        .build()
        .map_err(|e| e.to_string())?;

    let mut size = 10;

    let mut input_events = Vec::new();
    let mut entities = Vec::new();
    let mut events = sdl_context.event_pump()?;

    'mainloop: loop {
        for event in events.poll_iter() {
            match event {
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                }
                | Event::Quit { .. } => break 'mainloop,
                Event::MouseWheel { y, .. } if y < 0 => input_events.push(Input::ZoomOut),
                Event::MouseWheel { y, .. } if y > 0 => input_events.push(Input::ZoomIn),
                _ => {}
            }
        }
        system_input(&mut input_events, &mut entities);
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 255));
        canvas.clear();
        canvas.set_draw_color(Color::RGBA(255, 0, 0, 255));
        canvas.draw_rect(Rect::new(0, 0, size, size))?;
        canvas.present();
    }

    Ok(())
}

/// Move the camera
fn system_input(input_events: &mut Vec<Input>, entities: &mut Vec<Rect>) {
    while let Some(e) = input_events.pop() {
        println!("{:?}", e)
    }
}

/// Draw the entities based on camera location
fn system_render(canvas: &mut Canvas, entities: &mut Vec<Node>) {}
