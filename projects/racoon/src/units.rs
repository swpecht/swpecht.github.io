use bevy::prelude::*;

const PADDLE_COLOR: Color = Color::rgb(0.3, 0.3, 0.7);
const PADDLE_SIZE: Vec3 = Vec3::new(20.0, 20.0, 0.0);

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
                    scale: PADDLE_SIZE,
                    ..default()
                },
                sprite: Sprite {
                    color: PADDLE_COLOR,
                    ..default()
                },
                ..default()
            },
        }
    }
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
