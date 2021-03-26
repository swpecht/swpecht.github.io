use na::{Point3, Vector3};

use crate::rendering::Element;

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

#[derive(Copy, Clone)]
pub struct Sphere {
    pub center: Point3<f64>,
    pub radius: f64,
    pub color: Color,
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
}
