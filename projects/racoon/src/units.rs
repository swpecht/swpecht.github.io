use std::time::Duration;

use bevy::prelude::*;

const ENEMY_COLOR: Color = Color::rgb(0.3, 0.3, 0.7);
const ENEMY_SIZE: Vec3 = Vec3::new(20.0, 20.0, 0.0);

const SPAWN_COLOR: Color = Color::rgb(0.7, 0.3, 0.3);
const SPAWN_RANGE: f32 = 20.;

#[derive(Bundle)]
pub struct EnemyBundle {
    position: Position,
    shape: Shape,
    sprite: SpriteBundle,
}

impl EnemyBundle {
    pub fn new(pos: Vec2) -> Self {
        Self {
            position: Position { loc: pos },
            shape: Shape::Circle,
            sprite: SpriteBundle {
                transform: Transform {
                    translation: Vec3::new(pos.x, pos.y, 0.0),
                    scale: ENEMY_SIZE,
                    ..default()
                },
                sprite: Sprite {
                    color: ENEMY_COLOR,
                    ..default()
                },
                ..default()
            },
        }
    }
}

#[derive(Bundle)]
pub struct EnemySpawn {
    position: Position,
    sprite: SpriteBundle,
    spawner: Spawner,
}

impl EnemySpawn {
    pub fn new(pos: Vec2) -> Self {
        Self {
            position: Position { loc: pos },
            spawner: Spawner {
                spawner_type: SpawnerType::Enemy,
                range: SPAWN_RANGE,
                timer: Timer::new(Duration::from_secs(1), TimerMode::Repeating),
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

#[derive(Component)]
pub struct Spawner {
    pub spawner_type: SpawnerType,
    pub range: f32,
    pub timer: Timer,
}

pub enum SpawnerType {
    Enemy,
}

#[derive(Component)]
pub struct Position {
    pub loc: Vec2,
}

impl Position {
    pub fn loc(&self) -> &Vec2 {
        &self.loc
    }
}

#[derive(Component)]
enum Shape {
    Circle,
}
