use bevy::prelude::*;

use crate::physics::{Position, Velocity};

pub struct GraphicsPlugin {}

impl Plugin for GraphicsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_assets)
            .add_systems(Update, (position_render_system, animate_sprite))
            .add_systems(Update, direction_render_system);
    }
}

fn position_render_system(mut query: Query<(&Position, &mut Transform)>) {
    for (pos, mut transform) in &mut query {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
    }
}

fn direction_render_system(mut query: Query<(&Velocity, &mut Transform)>) {
    for (vel, mut transform) in &mut query {
        transform.rotation = Quat::default();
        transform.rotate(Quat::from_rotation_z(-vel.angle_between(Vec2::Y)))
    }
}

/// Used to help identify our main camera
#[derive(Component)]
pub struct MainCamera;

#[derive(Bundle)]
pub struct AnimatedSpriteBundle {
    animation_indices: AnimationIndices,
    timer: AnimationTimer,
    sprite_sheet: SpriteSheetBundle,
}

impl AnimatedSpriteBundle {
    pub fn new(texture_atlas_handle: Handle<TextureAtlas>, pos: Vec2) -> Self {
        let animation_indices = AnimationIndices { first: 0, last: 3 };

        Self {
            timer: AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
            sprite_sheet: SpriteSheetBundle {
                texture_atlas: texture_atlas_handle,
                sprite: TextureAtlasSprite::new(animation_indices.first),
                transform: Transform {
                    translation: Vec3::new(pos.x, pos.y, 0.0),
                    scale: Vec3::new(1.0, 1.0, 0.0),
                    ..default()
                },
                ..default()
            },
            animation_indices,
        }
    }
}

#[derive(Component)]
struct AnimationIndices {
    first: usize,
    last: usize,
}

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn animate_sprite(
    time: Res<Time>,
    mut query: Query<(
        &AnimationIndices,
        &mut AnimationTimer,
        &mut TextureAtlasSprite,
    )>,
) {
    for (indices, mut timer, mut sprite) in &mut query {
        timer.tick(time.delta());
        if timer.just_finished() {
            sprite.index = if sprite.index == indices.last {
                indices.first
            } else {
                sprite.index + 1
            };
        }
    }
}

fn load_assets() {}
