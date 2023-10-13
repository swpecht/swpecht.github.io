use std::time::Duration;

use bevy::prelude::*;

use crate::{
    graphics::AnimatedSpriteBundle,
    physics::{Position, Velocity},
};

const ENEMY_COLOR: Color = Color::rgb(0.3, 0.3, 0.7);
const ENEMY_SIZE: Vec3 = Vec3::new(20.0, 20.0, 0.0);

const SPAWN_COLOR: Color = Color::rgb(0.7, 0.3, 0.3);
const SPAWN_RANGE: f32 = 20.;

const UNIT_VELOCITY: f32 = 30.;

pub struct UnitsPlugin {}

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, unit_movement_system);
    }
}

#[derive(Bundle)]
pub struct EnemyBundle {
    position: Position,
    velocity: Velocity,
    shape: Shape,
    sprite: AnimatedSpriteBundle,
}

impl EnemyBundle {
    pub fn new(pos: Vec2, sprite: AnimatedSpriteBundle) -> Self {
        Self {
            position: Position(pos),
            shape: Shape::Circle,
            velocity: Velocity(Vec2 { x: 0., y: 0. }),
            sprite,
        }
    }
}

#[derive(Bundle)]
pub struct EnemySpawnBundle {
    position: Position,
    sprite: SpriteBundle,
    spawner: SpawnerBundle,
}

impl EnemySpawnBundle {
    pub fn new(pos: Vec2) -> Self {
        Self {
            position: Position(pos),
            spawner: SpawnerBundle {
                spawner_type: SpawnerType::Enemy,
                timer: SpawnerTimer(Timer::new(Duration::from_secs(1), TimerMode::Repeating)),
            },
            sprite: SpriteBundle {
                transform: Transform {
                    translation: Vec3::new(pos.x, pos.y, 0.0),
                    scale: ENEMY_SIZE,
                    ..default()
                },
                sprite: Sprite {
                    color: SPAWN_COLOR,
                    ..default()
                },
                ..default()
            },
        }
    }
}

// TODO: Change this to a bundle
#[derive(Bundle)]
pub struct SpawnerBundle {
    pub spawner_type: SpawnerType,
    pub timer: SpawnerTimer,
}

#[derive(Component, Deref, DerefMut)]
pub struct SpawnerTimer(Timer);

#[derive(Component)]
pub enum SpawnerType {
    Enemy,
}

#[derive(Component)]
enum Shape {
    Circle,
}

#[derive(Component, Default, Deref, DerefMut)]
pub struct GoalPos(pub Vec2);

fn unit_movement_system(mut query: Query<(&mut Velocity, &Position, &GoalPos)>) {
    for (mut vel, pos, gpos) in &mut query {
        *vel = Velocity((**gpos - **pos).normalize() * 30.);
    }
}
