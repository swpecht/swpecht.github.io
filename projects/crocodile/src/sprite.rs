use std::time::{Duration, Instant};

use bevy::{
    math::{vec2, vec3},
    prelude::*,
    render::camera::ScalingMode,
    time::Stopwatch,
};

use crate::{
    gamestate::{Action, SimId, SimState},
    ui::{ActionEvent, CurrentCharacter},
};

pub const TILE_SIZE: usize = 32;
const GRID_WIDTH: usize = 20;
const GRID_HEIGHT: usize = 20;

const TILE_LAYER: f32 = 0.0;
const CHAR_LAYER: f32 = 1.0;

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (setup_camera, sync_sim, setup_tiles))
            .add_systems(Update, (animate_sprite, process_curves, action_system));
    }
}

#[derive(Component, Clone)]
struct Curve {
    start: Vec2,
    end: Vec2,
    time: Stopwatch,
    duration: Duration,
}

impl Curve {
    fn cur_pos(&self) -> Vec2 {
        let t = (self.time.elapsed_secs() / self.duration.as_secs() as f32).min(1.0);
        self.start.lerp(self.end, t)
    }

    fn is_finished(&self) -> bool {
        self.time.elapsed() >= self.duration
    }
}

/// Used to help identify our main camera
#[derive(Component)]
pub struct MainCamera;

fn setup_camera(mut commands: Commands) {
    // Camera
    let mut camera_bundle = Camera2dBundle {
        transform: Transform::from_xyz(
            (GRID_WIDTH * TILE_SIZE / 2) as f32,
            (GRID_HEIGHT * TILE_SIZE / 2) as f32,
            0.0,
        ),
        ..default()
    };
    camera_bundle.projection.scaling_mode =
        ScalingMode::FixedVertical((GRID_HEIGHT * TILE_SIZE + TILE_SIZE) as f32);

    commands.spawn((camera_bundle, MainCamera));
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

impl crate::gamestate::Character {
    fn idle(&self) -> String {
        use crate::gamestate::Character::*;
        match self {
            Knight => "pixel-crawler/Heroes/Knight/Idle/Idle-Sheet.png".to_string(),
            Orc => "pixel-crawler/Enemy/Orc Crew/Orc/Idle/Idle-Sheet.png".to_string(),
        }
    }
}

fn sync_sim(
    mut commands: Commands,
    sim: Res<SimState>,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    for (id, loc, character) in sim
        .characters()
        .into_iter()
        .map(|(id, l, c)| (id, l.to_world(), c))
    {
        // TODO: add support for changing location of things if they're already spawned
        let texture = asset_server.load(character.idle());
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
                transform: Transform::from_translation(vec3(loc.x, loc.y, CHAR_LAYER)),
                ..default()
            },
            animation_indices,
            AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
            id,
        ));
    }
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
                    (r * TILE_SIZE) as f32,
                    (c * TILE_SIZE) as f32,
                    TILE_LAYER,
                )),
                ..default()
            });
        }
    }
}

/// Translate action events into the proper display within the game visualization
fn action_system(
    mut commands: Commands,
    mut ev_action: EventReader<ActionEvent>,
    query: Query<(Entity, &SimId, &Transform)>,
    mut sim: ResMut<SimState>,
    mut cur: ResMut<CurrentCharacter>,
) {
    for ev in ev_action.read() {
        debug!("action event received: {:?}", ev);
        sim.apply(ev.action);
        use Action::*;
        match ev.action {
            EndTurn => {} // todo
            Attack { dmg, range, aoe } => todo!(),
            MoveUp | MoveDown | MoveLeft | MoveRight => {
                handle_move(&mut commands, ev.action, &query, cur.0)
            }
        }
        cur.0 = sim.cur_char();
        debug!("{:?}", sim.cur_char());
    }
}

fn handle_move(
    commands: &mut Commands,
    action: Action,
    query: &Query<(Entity, &SimId, &Transform)>,
    cur: SimId,
) {
    query
        .iter()
        .filter(|(_, id, _)| **id == cur)
        .for_each(|(e, _, t)| {
            use Action::*;
            let offset = match action {
                MoveUp => vec2(0.0, TILE_SIZE as f32),
                MoveDown => vec2(0.0, -1.0 * TILE_SIZE as f32),
                MoveLeft => vec2(-1.0 * TILE_SIZE as f32, 0.0),
                MoveRight => vec2(TILE_SIZE as f32, 0.0),
                _ => panic!("invalid action passed to move handler"),
            };

            let start = vec2(t.translation.x, t.translation.y);
            let curve = Curve {
                start,
                end: start + offset,
                time: Stopwatch::new(),
                duration: Duration::from_secs(1),
            };
            commands.entity(e).insert(curve.clone());
        });
}

fn process_curves(
    mut commands: Commands,
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut query: Query<(Entity, &mut Transform, &mut Curve)>,
) {
    for (entity, mut transform, mut curve) in &mut query {
        gizmos.linestrip_2d([curve.start, curve.end], Color::WHITE);
        curve.time.tick(time.delta());
        let pos = curve.cur_pos();
        transform.translation = vec3(pos.x, pos.y, transform.translation.z);

        if curve.is_finished() {
            commands.entity(entity).remove::<Curve>();
        }
    }
}
