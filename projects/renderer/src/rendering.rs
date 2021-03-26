use na::{Point3, Vector3};

use crate::scene::{Color, Scene, Sphere};

pub struct Ray {
    pub origin: Point3<f64>,
    pub direction: Vector3<f64>,
}

/// Tracks where to render a view from
pub struct Camera {
    pub direction: Vector3<f64>,
    pub up: Vector3<f64>,
    pub right: Vector3<f64>,
    pub location: Point3<f64>,
}

impl Ray {
    /// Create primes
    /// And https://stackoverflow.com/questions/13078243/how-to-move-a-camera-using-in-a-ray-tracer
    pub fn create_prime(x: u32, y: u32, scene: &Scene, camera: &Camera) -> Ray {
        let normalized_x = 1.0 - (x as f64 / scene.width as f64) - 0.5;
        let normalized_y = (y as f64 / scene.height as f64) - 0.5;

        let direction: Vector3<f64> =
            normalized_x * camera.right + normalized_y * camera.up + camera.direction;
        Ray {
            origin: camera.location,
            direction: direction.normalize(),
        }
    }
}

pub enum Element {
    Sphere(Sphere),
}

impl Element {
    pub fn color(&self) -> &Color {
        match *self {
            Element::Sphere(ref s) => &s.color,
        }
    }
}

pub trait Intersectable {
    /// Returns distance to closest point of intersection.
    fn intersect(&self, ray: &Ray) -> Option<f64>;
    fn surface_normal(&self, hit_point: &Point3<f64>) -> Vector3<f64>;
}

impl Intersectable for Element {
    fn intersect(&self, ray: &Ray) -> Option<f64> {
        match *self {
            Element::Sphere(ref s) => s.intersect(ray),
        }
    }

    fn surface_normal(&self, hit_point: &Point3<f64>) -> Vector3<f64> {
        match *self {
            Element::Sphere(ref s) => s.surface_normal(hit_point),
        }
    }
}

impl Intersectable for Sphere {
    fn intersect(&self, ray: &Ray) -> Option<f64> {
        // Adapted from: https://bheisler.github.io/post/writing-raytracer-in-rust-part-1/

        //Create a line segment between the ray origin and the center of the sphere
        let l: Vector3<f64> = self.center - ray.origin;
        //Use l as a hypotenuse and find the length of the adjacent side
        let adj = l.dot(&ray.direction);
        //Find the length-squared of the opposite side
        //This is equivalent to (but faster than) (l.length() * l.length()) - (adj * adj)
        let d2 = l.dot(&l) - (adj * adj);
        let radius2 = self.radius * self.radius;
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

    fn surface_normal(&self, hit_point: &Point3<f64>) -> Vector3<f64> {
        (*hit_point - self.center).normalize()
    }
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

const BLACK: Color = Color { r: 0, g: 0, b: 0 };

pub fn cast_ray(scene: &Scene, ray: &Ray) -> Color {
    let mut closest_element: Option<&Element> = None;
    let mut closest_distance: Option<f64> = None;
    for element in scene.elements.iter() {
        let distance = element.intersect(&ray);
        if !distance.is_none() && (closest_distance.is_none() || distance < closest_distance) {
            closest_distance = distance;
            closest_element = Some(element);
        }
    }

    let color = if !closest_element.is_none() {
        get_color(
            &scene,
            &ray,
            closest_distance.unwrap(),
            closest_element.unwrap(),
        )
    } else {
        BLACK
    };

    return color;
}
