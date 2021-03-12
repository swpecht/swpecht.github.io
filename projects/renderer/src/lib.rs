use wasm_bindgen::prelude::*;
use wasm_bindgen::Clamped;
use wasm_bindgen::JsCast;

extern crate nalgebra as na;
use na::{Point3, Vector3};

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
}

impl Color {
    pub fn clamp(&self) -> Color {
        Color {
            r: self.r.min(255).max(0),
            b: self.b.min(255).max(0),
            g: self.g.min(255).max(0),
        }
    }
}

struct Ray {
    origin: Point3<f64>,
    direction: Vector3<f64>,
}

#[derive(Copy, Clone)]
struct Sphere {
    center: Point3<f64>,
    radius: f64,
    color: Color,
}

struct Light {
    direction: Vector3<f64>,
    intensity: f32,
}

struct Scene {
    width: u32,
    height: u32,
    light: Light,
    spheres: Vec<Sphere>,
    camera_direction: Vector3<f64>,
    camera_up: Vector3<f64>,
    camera_right: Vector3<f64>,
    camera_location: Point3<f64>,
}

#[wasm_bindgen]
pub struct TestStruct {
    x: f32,
}

#[wasm_bindgen]
impl TestStruct {
    pub fn new() -> TestStruct {
        TestStruct { x: 42.0 }
    }

    pub fn get(&self) -> f32 {
        return self.x;
    }
}

/// Create primes
/// And https://stackoverflow.com/questions/13078243/how-to-move-a-camera-using-in-a-ray-tracer
fn create_prime(x: u32, y: u32, scene: &Scene) -> Ray {
    let normalized_x = 1.0 - (x as f64 / scene.width as f64) - 0.5;
    let normalized_y = (y as f64 / scene.height as f64) - 0.5;

    let direction: Vector3<f64> =
        normalized_x * scene.camera_right + normalized_y * scene.camera_up + scene.camera_direction;

    Ray {
        origin: scene.camera_location,
        direction: direction.normalize(),
    }
}

/// Returns distance to closest point of intersection.
fn intersect(ray: &Ray, sphere: &Sphere) -> Option<f64> {
    // Adapted from: https://bheisler.github.io/post/writing-raytracer-in-rust-part-1/

    //Create a line segment between the ray origin and the center of the sphere
    let l: Vector3<f64> = sphere.center - ray.origin;
    //Use l as a hypotenuse and find the length of the adjacent side
    let adj = l.dot(&ray.direction);
    //Find the length-squared of the opposite side
    //This is equivalent to (but faster than) (l.length() * l.length()) - (adj * adj)
    let d2 = l.dot(&l) - (adj * adj);
    let radius2 = sphere.radius * sphere.radius;
    //If that length-squared is less than radius squared, the ray intersects the sphere
    if d2 > radius2 {
        return None;
    }
    let thc = (radius2 - d2).sqrt();
    let t0 = adj - thc;
    let t1 = adj + thc;

    if t0 < 0.0 && t1 < 0.0 {
        return None;
    }

    let distance = if t0 < t1 { t0 } else { t1 };
    Some(distance)
}

fn surface_normal(sphere: &Sphere, hit_point: &Point3<f64>) -> Vector3<f64> {
    (*hit_point - sphere.center).normalize()
}

fn get_color(scene: &Scene, ray: &Ray, distance: f64, sphere: &Sphere) -> Color {
    let hit_point = ray.origin + (ray.direction * distance);
    let surface_normal = surface_normal(sphere, &hit_point);
    let direction_to_light = -scene.light.direction.normalize();
    let light_power =
        (surface_normal.dot(&direction_to_light) as f32).max(0.0) * scene.light.intensity;
    const ALBEDO: f32 = 0.18; // placeholder
    let light_reflected = ALBEDO / std::f32::consts::PI;

    let color = Color {
        r: (sphere.color.r as f32 * light_power * light_reflected) as u8,
        g: (sphere.color.g as f32 * light_power * light_reflected) as u8,
        b: (sphere.color.b as f32 * light_power * light_reflected) as u8,
    };
    color.clamp()
}

/// Renders frame with camera at specied point arount a circle
#[wasm_bindgen]
pub fn render(angle: f64) {
    let scene = create_scene(angle);

    // Array for RGBA values
    let mut pixels = vec![0u8; (scene.width * scene.height * 4) as usize];

    for x in 0..scene.width {
        for y in 0..scene.height {
            let ray = create_prime(x, y, &scene);
            let index = (x + y * scene.width) as usize;
            pixels[4 * index + 3] = 255; // Set background to not be transparent
            for sphere in scene.spheres.iter() {
                let distance = intersect(&ray, &sphere);
                if !distance.is_none() {
                    let color = get_color(&scene, &ray, distance.unwrap(), sphere);
                    pixels[4 * index] = color.r;
                    pixels[4 * index + 1] = color.g;
                    pixels[4 * index + 2] = color.b;
                    pixels[4 * index + 3] = 255; // no transparency
                    break;
                }
            }
        }
    }

    paint(Clamped(&pixels), scene);
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

    let sphere1 = Sphere {
        center: Point3::new(0.0, 0.0, -2.0),
        radius: 0.5,
        color: red,
    };

    let sphere2 = Sphere {
        center: Point3::new(-1.0, 0.0, -3.0),
        color: green,
        ..sphere1
    };

    let sphere3 = Sphere {
        center: Point3::new(1.5, 0.0, -4.0),
        color: blue,
        ..sphere1
    };

    // Calculate camera postion as point on a circle
    // Adapted from: https://stackoverflow.com/questions/839899/how-do-i-calculate-a-point-on-a-circle-s-circumference
    let radius = 5.0; // Radius of orbit
    let orbit_center = Point3::new(0.0, 0.0, -3.0);
    let x = orbit_center.x + radius * angle.cos();
    let z = orbit_center.z + radius * angle.sin();
    let camera_location = Point3::new(x, 0.0, z);
    log(&format!("x={}, z={}", x, z));

    // From: https://stackoverflow.com/questions/13078243/how-to-move-a-camera-using-in-a-ray-tracer
    let camera_direction = Vector3::new(
        orbit_center.x - camera_location.x,
        orbit_center.y - camera_location.y,
        orbit_center.z - camera_location.z,
    )
    .normalize();
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
        spheres: vec![sphere1, sphere2, sphere3],
        camera_direction: camera_direction,
        camera_location: camera_location,
        camera_right: camera_right,
        camera_up: camera_up,
    };

    return scene;
}
