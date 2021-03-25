use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;

extern crate nalgebra as na;
use na::{Point3, Vector3};
use web_sys::ImageData;

mod rendering;
mod scene;

use rendering::Ray;
use scene::{Color, Element, Intersectable, Light, Scene, Sphere};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    fn log_u8(a: u8);
}

/// Renders frame with camera at specied point arount a circle
#[wasm_bindgen]
pub fn render(angle: f64) {
    let scene = create_scene(angle);

    // Array for RGBA values
    let mut pixels = vec![0u8; (scene.width * scene.height * 4) as usize];

    for x in 0..scene.width {
        for y in 0..scene.height {
            let ray = scene.create_prime(x, y);
            let index = (x + y * scene.width) as usize;
            pixels[4 * index + 3] = 255; // Set background to not be transparent

            let mut closest_element: Option<&Element> = None;
            let mut closest_distance: Option<f64> = None;
            for element in scene.elements.iter() {
                let distance = element.intersect(&ray);
                if !distance.is_none()
                    && (closest_distance.is_none() || distance < closest_distance)
                {
                    closest_distance = distance;
                    closest_element = Some(element);
                }
            }

            if !closest_element.is_none() {
                let color = get_color(
                    &scene,
                    &ray,
                    closest_distance.unwrap(),
                    closest_element.unwrap(),
                );
                pixels[4 * index] = color.r;
                pixels[4 * index + 1] = color.g;
                pixels[4 * index + 2] = color.b;
                pixels[4 * index + 3] = 255; // no transparency
            }
        }
    }

    paint(Clamped(&pixels), scene);
}

fn get_color(scene: &Scene, ray: &Ray, distance: f64, element: &Element) -> Color {
    let hit_point = ray.origin + (ray.direction * distance);
    let surface_normal = element.surface_normal(&hit_point);
    let direction_to_light = -scene.light.direction.normalize();
    let light_power =
        (surface_normal.dot(&direction_to_light) as f32).max(0.0) * scene.light.intensity;
    const ALBEDO: f32 = 0.18; // placeholder
    let light_reflected = ALBEDO / std::f32::consts::PI;

    let color = Color {
        r: (element.color().r as f32 * light_power * light_reflected) as u8,
        g: (element.color().g as f32 * light_power * light_reflected) as u8,
        b: (element.color().b as f32 * light_power * light_reflected) as u8,
    };
    color.clamp()
}

/// Paint pixels to canvas
fn paint(pixels: wasm_bindgen::Clamped<&[u8]>, scene: Scene) {
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

    let image_data =
        ImageData::new_with_u8_clamped_array_and_sh(pixels, scene.width, scene.height).unwrap();

    context.put_image_data(&image_data, 0.0, 0.0).unwrap();
}

/// Create a basic scene with a few objects and some lighting.
fn create_scene(angle: f64) -> Scene {
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

    // Calculate camera postion as point on a circle
    // Adapted from: https://stackoverflow.com/questions/839899/how-do-i-calculate-a-point-on-a-circle-s-circumference
    let radius = 5.0; // Radius of orbit
    let orbit_center = Point3::new(0.0, 0.0, -3.0);
    let x = orbit_center.x + radius * angle.cos();
    let z = orbit_center.z + radius * angle.sin();
    let camera_location = Point3::new(x, 0.0, z);
    log(&format!("x={}, z={}", x, z));

    // From: https://stackoverflow.com/questions/13078243/how-to-move-a-camera-using-in-a-ray-tracer
    let camera_direction: Vector3<f64> = (orbit_center - camera_location).normalize();
    log(&format!(
        "direction: {}, {}",
        camera_direction.x, camera_direction.z
    ));
    let initial_camera_up = Vector3::new(0.0, 1.0, 0.0);
    let camera_right = initial_camera_up.cross(&camera_direction);
    let camera_up = camera_right.cross(&camera_direction);

    let scene = Scene {
        width: 500,
        height: 500,
        light: Light {
            direction: Vector3::new(1.0, -1.0, 0.0),
            intensity: 30.0,
        },
        elements: vec![sphere1, sphere2, sphere3],
        camera_direction: camera_direction,
        camera_location: camera_location,
        camera_right: camera_right,
        camera_up: camera_up,
    };

    return scene;
}
