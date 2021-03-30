use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;

extern crate nalgebra as na;
use na::{Point3, Vector3};
use web_sys::ImageData;

mod rendering;
mod scene;

use rendering::{Camera, Element, Ray};
use scene::{Color, Light, Scene, Sphere};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u8(a: u8);
}

#[wasm_bindgen]
pub struct PixelBuffer {
    pixels: std::vec::Vec<u8>,
}

#[wasm_bindgen]
impl PixelBuffer {
    pub fn new(universe: &Universe) -> PixelBuffer {
        let pixels = vec![0u8; (universe.scene.width * universe.scene.height * 4) as usize];
        return PixelBuffer { pixels: pixels };
    }
}

#[wasm_bindgen]
pub struct Universe {
    scene: Scene,
}

#[wasm_bindgen]
impl Universe {
    pub fn new() -> Universe {
        let scene = create_scene();
        return Universe { scene: scene };
    }

    /// Renders frame with camera at specied point arount a circle
    pub fn render(&mut self, angle: f64, pixel_buffer: &mut PixelBuffer) {
        let camera = create_camera(angle);
        for y in 0..self.scene.height {
            let start_index = (y * self.scene.width * 4) as usize;
            let end_index = ((y + 1) * self.scene.width * 4) as usize;
            let pixel_column = &mut pixel_buffer.pixels[start_index..end_index];
            for (x, chunk) in pixel_column.chunks_mut(4).enumerate() {
                render_pixel(x as u32, y, chunk, &self.scene, &camera);
            }

            pixel_column
                .chunks_mut(4)
                .enumerate()
                .for_each(|(x, chunk)| render_pixel(x as u32, y, chunk, &self.scene, &camera));
        }
    }
}

fn render_pixel(x: u32, y: u32, pixel_chunk: &mut [u8], scene: &Scene, camera: &Camera) {
    let ray = Ray::create_prime(x, y, scene, camera);
    let color = rendering::cast_ray(scene, &ray);
    pixel_chunk[0] = color.r;
    pixel_chunk[1] = color.g;
    pixel_chunk[2] = color.b;
    pixel_chunk[3] = 255u8; // no transparency
}

/// Paint pixels to a canvas
#[wasm_bindgen]
pub struct Painter {
    canvas: web_sys::CanvasRenderingContext2d,
}

#[wasm_bindgen]
impl Painter {
    pub fn new() -> Painter {
        Painter {
            canvas: get_canvas(),
        }
    }

    pub fn paint(&self, universe: &Universe, pixel_buffer: &mut PixelBuffer) {
        let pixels = &pixel_buffer.pixels;
        let scene = &universe.scene;
        let image_data =
            ImageData::new_with_u8_clamped_array_and_sh(Clamped(pixels), scene.width, scene.height)
                .unwrap();

        self.canvas.put_image_data(&image_data, 0.0, 0.0).unwrap();
    }
}

fn get_canvas() -> web_sys::CanvasRenderingContext2d {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document.get_element_by_id("canvas").unwrap();
    let canvas: web_sys::HtmlCanvasElement = canvas
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| ())
        .unwrap();

    return canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .unwrap();
}

/// Create a basic scene with a few objects and some lighting.
fn create_scene() -> Scene {
    let red = Color { r: 200, g: 0, b: 0 };

    let green = Color { r: 0, g: 200, b: 0 };

    let blue = Color { r: 0, g: 0, b: 200 };

    let sphere1 = Element::Sphere(Sphere {
        center: Point3::new(0.0, 0.0, -2.0),
        radius: 0.5,
        color: red,
    });

    let sphere2 = Element::Sphere(Sphere {
        center: Point3::new(-1.0, 0.0, -3.0),
        radius: 0.5,
        color: green,
    });

    let sphere3 = Element::Sphere(Sphere {
        center: Point3::new(1.5, 0.0, -4.0),
        radius: 0.5,
        color: blue,
    });

    let scene = Scene {
        width: 500,
        height: 500,
        light: Light {
            direction: Vector3::new(1.0, -1.0, 0.0),
            intensity: 30.0,
        },
        elements: vec![sphere1, sphere2, sphere3],
    };
    return scene;
}

// Create a camera that rotates around the scene
fn create_camera(angle: f64) -> Camera {
    // Calculate camera postion as point on a circle
    // Adapted from: https://stackoverflow.com/questions/839899/how-do-i-calculate-a-point-on-a-circle-s-circumference
    let radius = 5.0; // Radius of orbit
    let orbit_center = Point3::new(0.0, 0.0, -3.0);
    let x = orbit_center.x + radius * angle.cos();
    let z = orbit_center.z + radius * angle.sin();
    let camera_location = Point3::new(x, 0.0, z);

    // From: https://stackoverflow.com/questions/13078243/how-to-move-a-camera-using-in-a-ray-tracer
    let camera_direction: Vector3<f64> = (orbit_center - camera_location).normalize();
    let initial_camera_up = Vector3::new(0.0, 1.0, 0.0);
    let camera_right = initial_camera_up.cross(&camera_direction);
    let camera_up = camera_right.cross(&camera_direction);

    let camera = Camera {
        direction: camera_direction,
        location: camera_location,
        right: camera_right,
        up: camera_up,
    };
    return camera;
}
