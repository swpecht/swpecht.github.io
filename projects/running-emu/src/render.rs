use hecs::World;
use sdl2::{
    gfx::primitives::DrawRenderer,
    pixels::{self, Color},
    rect::Rect,
    render::Canvas,
    video::Window,
    VideoSubsystem,
};

use crate::{build_char_output, get_max_point, spatial::Point};

const SCREEN_WIDTH: i16 = 1280;
const SCREEN_HEIGHT: i16 = 720;

pub fn system_render(world: &World, canvas: &mut Canvas<Window>) -> Result<(), String> {
    // Draw grid
    let max_p = get_max_point(world);
    let width = max_p.x as i16;
    let height = max_p.y as i16;

    let char_buffer = build_char_output(world);

    for y in 0..max_p.y {
        for x in 0..max_p.x {
            let p = Point { x: x, y: y };
            draw_char_center(canvas, p, char_buffer[y][x], max_p)?;
        }
    }

    draw_grid(canvas, width, height)?;

    Ok(())
}

fn draw_char_center(
    canvas: &mut Canvas<Window>,
    p: Point,
    c: char,
    max_p: Point,
) -> Result<(), String> {
    let (canvas_width, canvas_height) = canvas.window().drawable_size();
    let box_width = canvas_width / max_p.x as u32;
    let box_height = canvas_height / max_p.y as u32;

    let x = p.x as u32 * box_width + box_width / 2;
    let y = p.y as u32 * box_height + box_height / 2;

    match c {
        '?' => canvas.set_draw_color(Color::BLACK),
        _ => canvas.set_draw_color(Color::BLUE),
    }

    canvas.fill_rect(Rect::new(
        (p.x as u32 * box_width) as i32,
        (p.y as u32 * box_height) as i32,
        box_width,
        box_height,
    ))?;
    canvas.character(x as i16, y as i16, c, Color::WHITE)?;

    Ok(())
}

fn draw_grid(canvas: &mut Canvas<Window>, width: i16, height: i16) -> Result<(), String> {
    let box_width = SCREEN_WIDTH / width;
    let box_height = SCREEN_HEIGHT / height;

    for r in 0..height {
        canvas.line(
            0,
            r * box_height,
            SCREEN_WIDTH,
            r * box_height,
            Color::WHITE,
        )?;
    }

    for c in 0..width {
        canvas.line(c * box_width, 0, c * box_width, SCREEN_HEIGHT, Color::WHITE)?;
    }

    Ok(())
}

pub fn get_window(video_subsys: VideoSubsystem) -> Result<Window, String> {
    let window = video_subsys
        .window("Running Emu", SCREEN_WIDTH as u32, SCREEN_HEIGHT as u32)
        .position_centered()
        .opengl()
        .build()
        .map_err(|e| e.to_string())?;

    Ok(window)
}
