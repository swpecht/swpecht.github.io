use na::{Point3, Vector3};

use crate::rendering::Ray;

#[derive(Copy, Clone)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
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

#[derive(Copy, Clone)]
pub struct Sphere {
    pub center: Point3<f64>,
    pub radius: f64,
    pub color: Color,
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

pub struct Light {
    pub direction: Vector3<f64>,
    pub intensity: f32,
}

pub struct Scene {
    pub width: u32,
    pub height: u32,
    pub light: Light,
    pub elements: Vec<Element>,
    pub camera_direction: Vector3<f64>,
    pub camera_up: Vector3<f64>,
    pub camera_right: Vector3<f64>,
    pub camera_location: Point3<f64>,
}

impl Scene {
    /// Create primes
    /// And https://stackoverflow.com/questions/13078243/how-to-move-a-camera-using-in-a-ray-tracer
    pub fn create_prime(&self, x: u32, y: u32) -> Ray {
        let normalized_x = 1.0 - (x as f64 / self.width as f64) - 0.5;
        let normalized_y = (y as f64 / self.height as f64) - 0.5;

        let direction: Vector3<f64> = normalized_x * self.camera_right
            + normalized_y * self.camera_up
            + self.camera_direction;
        Ray {
            origin: self.camera_location,
            direction: direction.normalize(),
        }
    }
}
