use bevy::{prelude::*, sprite::SpriteBundle};

use crate::units::Position;

pub struct WorldRenderPlugin {}

impl Plugin for WorldRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, position_render_system);
    }
}

fn position_render_system(mut query: Query<(&Position, &mut Transform)>) {
    for (pos, mut transform) in &mut query {
        transform.translation.x = pos.loc().x;
        transform.translation.y = pos.loc().y;
    }
}
