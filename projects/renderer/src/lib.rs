use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;

use web_sys::ImageData;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u8(a: u8);
}

#[derive(Copy, Clone)]
struct Color {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Copy, Clone)]
struct Sphere {
    x: f32,
    y: f32,
    z: f32,
    radius: f32,
    color: Color,
}

fn is_intersect(x: f32, y: f32, sphere: &Sphere) -> bool {
    let distance = ((x - sphere.x).powi(2) + (y - sphere.y).powi(2)).sqrt();
    return distance <= sphere.radius;
}

#[wasm_bindgen(start)]
pub fn start() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| ())
        .unwrap();

    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();

    let red = Color {
        r: 200,
        g: 0,
        b: 0,
        a: 255,
    };

    let green = Color {
        r: 0,
        g: 200,
        b: 0,
        a: 255,
    };

    let blue = Color {
        r: 0,
        g: 0,
        b: 200,
        a: 255,
    };

    let sphere1 = Sphere {
        x: 0.0,
        y: 0.0,
        z: 50.0,
        radius: 50.0,
        color: red,
    };

    let sphere2 = Sphere {
        x: -125.0,
        z: 75.0,
        color: green,
        ..sphere1
    };

    let sphere3 = Sphere {
        x: 125.0,
        z: 100.0,
        color: blue,
        ..sphere1
    };

    let objects = [sphere1, sphere2, sphere3];

    // Create Canvas, centered at (0, 0, 0)
    let width: u32 = 500;
    let height: u32 = 500;
    // Array for RGBA values
    let mut pixels = vec![0u8; (width * height * 4) as usize];

    for x_offset in 0..width {
        for y_offset in 0..height {
            let x = -255 + x_offset as i32;
            let y = -255 + y_offset as i32;
            for sphere in objects.iter() {
                if is_intersect(x as f32, y as f32, &sphere) {
                    let index = (x_offset + y_offset * width) as usize;
                    let color = &sphere.color;
                    pixels[4 * index] = color.r;
                    pixels[4 * index + 1] = color.g;
                    pixels[4 * index + 2] = color.b;
                    pixels[4 * index + 3] = color.a;
                    break;
                }
            }
        }
    }

    let image_data =
        ImageData::new_with_u8_clamped_array_and_sh(Clamped(&mut pixels), width, height).unwrap();

    context.put_image_data(&image_data, 0.0, 0.0).unwrap();
}
