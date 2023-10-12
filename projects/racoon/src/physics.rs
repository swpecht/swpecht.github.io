use bevy::prelude::*;

pub struct PhyscisPlugin {}

impl Plugin for PhyscisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedUpdate, velocity_system);
    }
}

#[derive(Component, Deref, DerefMut)]
pub struct Position(pub Vec2);
#[derive(Component, Deref, DerefMut)]
pub struct Velocity(pub Vec2);

fn velocity_system(mut query: Query<(&mut Position, &Velocity)>, time: Res<Time>) {
    let delta = time.delta().as_secs_f32();
    for (mut pos, vel) in &mut query {
        pos.x += vel.x * delta;
        pos.y += vel.y * delta;
    }
}
