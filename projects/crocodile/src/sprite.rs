use bevy::{math::vec3, prelude::*};

use crate::ui::ActionEvent;

const TILE_SIZE: usize = 32;
const GRID_WIDTH: usize = 20;
const GRID_HEIGHT: usize = 20;

const TILE_LAYER: f32 = 0.0;
const CHAR_LAYER: f32 = 1.0;

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_camera, setup, setup_tiles))
            .add_systems(Update, (animate_sprite, movement_system, action_system));
    }
}

/// This will be used to identify the main player entity
#[derive(Component)]
struct Player;

#[derive(Component)]
struct Curve {
    start: Vec2,
    end: Vec2,
}

impl Curve {
    fn lerp(&self, t: f32) -> Vec2 {
        self.start.lerp(self.end, t)
    }
}

fn setup_camera(mut commands: Commands) {
    // Camera
    commands.spawn(Camera2dBundle {
        transform: Transform::from_xyz(
            (GRID_WIDTH * TILE_SIZE / 2) as f32,
            (GRID_HEIGHT * TILE_SIZE / 2) as f32,
            0.0,
        ),
        ..default()
    });
}

fn movement_system(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &Curve)>,
    mut gizmos: Gizmos,
) {
    for (mut transform, cubic_curve) in &mut query {
        // Draw the curve
        gizmos.linestrip_2d([cubic_curve.start, cubic_curve.end], Color::WHITE);
        // position takes a point from the curve where 0 is the initial point
        // and 1 is the last point
        let t = (time.elapsed_seconds().sin() + 1.) / 2.;
        let pos = cubic_curve.lerp(t);
        transform.translation = vec3(pos.x, pos.y, transform.translation.z);
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
    mut query: Query<(&AnimationIndices, &mut AnimationTimer, &mut TextureAtlas)>,
) {
    for (indices, mut timer, mut atlas) in &mut query {
        timer.tick(time.delta());
        if timer.just_finished() {
            atlas.index = if atlas.index == indices.last {
                indices.first
            } else {
                atlas.index + 1
            };
        }
    }
}

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("pixel-crawler/Heroes/Knight/Idle/Idle-Sheet.png");
    let layout = TextureAtlasLayout::from_grid(Vec2::new(32.0, 32.0), 4, 1, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);
    // Use only the subset of sprites in the sheet that make up the run animation
    let animation_indices = AnimationIndices { first: 0, last: 3 };
    commands.spawn((
        SpriteSheetBundle {
            texture,
            atlas: TextureAtlas {
                layout: texture_atlas_layout,
                index: animation_indices.first,
            },
            transform: Transform::from_translation(vec3(0.0, 0.0, CHAR_LAYER)),
            ..default()
        },
        animation_indices,
        AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
        Player,
    ));
}

fn setup_tiles(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("pixel-crawler/Environment/Green Woods/Assets/Tiles.png");
    let layout = TextureAtlasLayout::from_grid(Vec2::new(32.0, 32.0), 4, 4, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    for r in 0..GRID_WIDTH {
        for c in 0..GRID_HEIGHT {
            commands.spawn(SpriteSheetBundle {
                texture: texture.clone(),
                atlas: TextureAtlas {
                    layout: texture_atlas_layout.clone(),
                    index: 5,
                },
                transform: Transform::from_translation(vec3(
                    (r * GRID_WIDTH) as f32,
                    (c * GRID_HEIGHT) as f32,
                    TILE_LAYER,
                )),
                ..default()
            });
        }
    }
}

/// Translate action events into the proper display within the game visualization
fn action_system(mut ev_levelup: EventReader<ActionEvent>) {
    for ev in ev_levelup.read() {
        debug!("action event received: {:?}", ev)
    }

    // let curve = Curve {
    //     start: vec2(0.0, 0.0),
    //     end: vec2(10.0, 0.0),
    // };
    // let entity_id = query_player.single();
    // commands.entity(entity_id).insert(curve);
}
