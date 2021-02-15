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

    let width: u32 = 255;
    let height: u32 = 255;
    // Array for RGBA values
    let mut pixels = vec![100u8; (width * height * 4) as usize];

    // https://developer.mozilla.org/en-US/docs/Web/API/ImageData/data
    for x in 0..width {
        for y in 0..height {
            let index = (x + y * width) as usize;
            pixels[4 * index] = x as u8;
            pixels[4 * index + 1] = y as u8;
            pixels[4 * index + 2] = 255 - (x as u8);
            pixels[4 * index + 3] = 255;
        }
    }

    let image_data =
        ImageData::new_with_u8_clamped_array_and_sh(Clamped(&mut pixels), width, height).unwrap();

    context.put_image_data(&image_data, 0.0, 0.0).unwrap();
}
