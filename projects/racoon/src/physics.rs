use bevy::prelude::*;

use crate::simulation::{Coordinates, SIMULATION_HEIGHT, SIMULATION_WIDTH};

const GRID_SIZE: f32 = 50.;

const MAX_LEFT: f32 = -1. * (SIMULATION_WIDTH as f32 / 2. * GRID_SIZE);
const MAX_BOTTOM: f32 = -1. * (SIMULATION_HEIGHT as f32 / 2. * GRID_SIZE);

pub struct PhyscisPlugin {}

impl Plugin for PhyscisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, velocity_system);
    }
}

#[derive(Component, Deref, DerefMut, Default)]
pub struct Position(pub Vec2);
#[derive(Component, Deref, DerefMut, Default)]
pub struct Velocity(pub Vec2);

fn velocity_system(mut query: Query<(&mut Position, &Velocity)>, time: Res<Time>) {
    let delta = time.delta().as_secs_f32();
    for (mut pos, vel) in &mut query {
        pos.x += vel.x * delta;
        pos.y += vel.y * delta;
    }
}

impl From<Coordinates> for Position {
    /// Translates the simulation coordinate into a world location, specifically
    /// it is the center of the location
    fn from(value: Coordinates) -> Self {
        Position(Vec2::from(value))
    }
}

impl From<Coordinates> for Vec2 {
    /// Translates the simulation coordinate into a world location, specifically
    /// it is the center of the location
    fn from(value: Coordinates) -> Self {
        let x = MAX_LEFT + value.x as f32 * GRID_SIZE;
        let y = MAX_BOTTOM + value.y as f32 * GRID_SIZE;
        Vec2 { x, y }
    }
}
