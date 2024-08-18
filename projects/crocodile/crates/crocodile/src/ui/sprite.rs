use bevy::{
    math::{vec2, vec3},
    prelude::*,
    render::camera::ScalingMode,
    time::Stopwatch,
};
use serde::Deserialize;

use crate::{
    ai::find_best_move,
    gamestate::{Action, SimCoords, SimId, SimState, Team},
    sim::info::Ability,
    ui::{ActionEvent, CurrentCharacter},
    PlayState,
};

pub const TILE_SIZE: usize = 32;
const GRID_WIDTH: usize = 20;
const GRID_HEIGHT: usize = 20;

const TILE_LAYER: f32 = 0.0;
const CHAR_LAYER: f32 = 1.0;
const PROJECTILE_LAYER: f32 = 2.0;

const HEALTH_BAR_COLOR: Color = Color::srgb(1.0, 0.0, 0.0);

const IDLE_LOCATION: &str = "/Idle/Idle-Sheet.png";

#[derive(Component, Clone)]
pub struct Curve {
    path: Vec<Vec2>,
    time: Stopwatch,
    speed: f32,
}

#[derive(Component)]
pub(super) struct AnimationIndices {
    first: usize,
    last: usize,
}

#[derive(Component, Deref, DerefMut)]
pub(super) struct AnimationTimer(Timer);

#[derive(Event, Debug)]
pub(super) struct SpawnProjectileEvent {
    start: Vec2,
    target: Vec2,
}

#[derive(Component)]
pub(super) struct Projectile;

#[derive(Debug, Clone, Hash, Deserialize)]
pub enum CharacterSprite {
    Skeleton,
    Knight,
    Orc,
    Wizard,
}

impl Curve {
    fn cur_pos(&self) -> Vec2 {
        // let t = (self.time.elapsed_secs() / self.duration.as_secs() as f32).min(1.0);

        let mut accumulated_time: f32 = 0.0;
        for i in 0..self.path.len() - 1 {
            let segment_distance = self.path[i].distance(self.path[i + 1]);
            let full_segment_time = segment_distance / self.speed;
            if accumulated_time + full_segment_time <= self.time.elapsed_secs() {
                accumulated_time += full_segment_time; // we're in the next segment
            } else {
                // we're in this segment
                let traveled_segment_time = self.time.elapsed_secs() - accumulated_time;
                let t = traveled_segment_time / full_segment_time;
                return self.path[i].lerp(self.path[i + 1], t);
            }
        }

        // if no other matches, we're at the end of the path
        return *self.path.last().unwrap();
    }

    fn is_finished(&self) -> bool {
        let mut total_distance = 0.0;
        for i in 0..self.path.len() - 1 {
            total_distance += self.path[i].distance(self.path[i + 1]);
        }
        let total_time = total_distance / self.speed;
        self.time.elapsed_secs() >= total_time
    }
}

/// Used to help identify our main camera
#[derive(Component)]
pub struct MainCamera;

pub(super) fn setup_camera(mut commands: Commands) {
    // Camera
    let mut camera_bundle = Camera2dBundle {
        transform: Transform::from_xyz(
            ((GRID_WIDTH + 1) * TILE_SIZE / 2) as f32,
            ((GRID_HEIGHT + 1) * TILE_SIZE / 2) as f32,
            0.0,
        ),
        ..default()
    };
    camera_bundle.projection.scaling_mode =
        ScalingMode::FixedVertical((GRID_HEIGHT * TILE_SIZE + TILE_SIZE) as f32);

    commands.spawn((camera_bundle, MainCamera));
}

pub(super) fn animate_sprite(
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

pub(super) fn sync_sim(
    mut commands: Commands,
    sim: Res<SimState>,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    spawned_entities: Query<Entity, With<SimId>>,
) {
    // delete everything that's already spawned
    for entity in &spawned_entities {
        commands.entity(entity).despawn_recursive();
    }

    // respawn everything
    for (id, loc, character) in sim
        .characters()
        .into_iter()
        .map(|(id, l, c)| (id, l.to_world(), c))
    {
        // TODO: add support for changing location of things if they're already spawned
        let texture = asset_server.load(character.sprite.idle());
        let layout = TextureAtlasLayout::from_grid(UVec2::new(32, 32), 4, 1, None, None);
        let texture_atlas_layout = texture_atlas_layouts.add(layout);
        // Use only the subset of sprites in the sheet that make up the run animation
        let animation_indices = AnimationIndices { first: 0, last: 3 };

        let health = Health {
            cur: sim.health(&id).unwrap(),
            max: sim.max_health(&id).unwrap(),
        };

        commands.spawn((
            SpriteBundle {
                texture,
                transform: Transform::from_translation(vec3(loc.x, loc.y, CHAR_LAYER)),
                ..default()
            },
            TextureAtlas {
                layout: texture_atlas_layout,
                index: animation_indices.first,
            },
            animation_indices,
            AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
            health,
            id,
        ));
    }
}

pub(super) fn setup_tiles(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    let texture = asset_server.load("pixel-crawler/Environment/Green Woods/Assets/Tiles.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(32, 32), 4, 4, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    for r in 0..GRID_WIDTH + 1 {
        for c in 0..GRID_HEIGHT + 1 {
            commands.spawn((
                SpriteBundle {
                    texture: texture.clone(),
                    transform: Transform::from_translation(vec3(
                        (r * TILE_SIZE) as f32,
                        (c * TILE_SIZE) as f32,
                        TILE_LAYER,
                    )),
                    ..default()
                },
                TextureAtlas {
                    layout: texture_atlas_layout.clone(),
                    index: 5,
                },
            ));
        }
    }
}

/// Translate action events into the proper display within the game visualization
pub(super) fn action_system(
    mut commands: Commands,
    mut ev_action: EventReader<ActionEvent>,
    mut ev_projectile: EventWriter<SpawnProjectileEvent>,
    query: Query<(Entity, &SimId, &Transform)>,
    mut sim: ResMut<SimState>,
    mut cur: ResMut<CurrentCharacter>,
    mut next_state: ResMut<NextState<PlayState>>,
) {
    let cur_char_pos = query
        .iter()
        .filter(|(_, id, _)| **id == cur.0)
        .map(|(_, _, t)| vec2(t.translation.x, t.translation.y))
        .next()
        .unwrap();

    for ev in ev_action.read() {
        debug!("action event received: {:?}", ev);
        sim.apply(ev.action);
        use Action::*;
        match ev.action {
            EndTurn => next_state.set(PlayState::Processing),
            Move { target } => handle_move(&mut commands, target, &query, cur.0),
            UseAbility {
                target,
                ability: Ability::BowAttack | Ability::LightCrossbow,
            } => {
                ev_projectile.send(SpawnProjectileEvent {
                    start: cur_char_pos,
                    target: target.to_world(),
                });
            }
            UseAbility {
                target,
                ability: Ability::MeleeAttack | Ability::Longsword | Ability::Ram | Ability::Charge, // todo fix this
            } => handle_melee(&mut commands, target, &query, cur.0),
        }
        cur.0 = sim.cur_char();
        debug!("{:?}", sim.cur_char());
    }
}

pub(super) fn handle_move(
    commands: &mut Commands,
    target: SimCoords,
    query: &Query<(Entity, &SimId, &Transform)>,
    cur: SimId,
) {
    query
        .iter()
        .filter(|(_, id, _)| **id == cur)
        .for_each(|(e, _, t)| {
            let start = vec2(t.translation.x, t.translation.y);
            let curve = Curve {
                path: vec![start, target.to_world()],
                time: Stopwatch::new(),
                speed: 64.0,
            };
            commands.entity(e).insert(curve.clone());
        });
}

fn handle_melee(
    commands: &mut Commands,
    target: SimCoords,
    query: &Query<(Entity, &SimId, &Transform)>,
    cur: SimId,
) {
    query
        .iter()
        .filter(|(_, id, _)| **id == cur)
        .for_each(|(e, _, t)| {
            let start = t.translation.truncate();
            let curve = Curve {
                path: vec![start, target.to_world().lerp(start, 0.5), start],
                time: Stopwatch::new(),
                speed: 128.0,
            };
            commands.entity(e).insert(curve.clone());
        });
}

pub(super) fn spawn_projectile(
    mut commands: Commands,
    mut ev_action: EventReader<SpawnProjectileEvent>,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    for ev in ev_action.read() {
        debug!("spawning projectile: {:?}", ev);
        let texture = asset_server.load("pixel-crawler/Weapons/Wood/Wood.png");
        let mut layout = TextureAtlasLayout::new_empty(UVec2::new(192, 112));
        layout.add_texture(URect::from_corners(UVec2::new(32, 0), UVec2::new(48, 16)));
        let texture_atlas_layout = texture_atlas_layouts.add(layout);
        let angle = (ev.target - ev.start).angle_between(ev.target);
        let mut transform = Transform::from_xyz(ev.start.x, ev.start.y, PROJECTILE_LAYER);
        transform.rotation = Quat::from_rotation_z(angle);

        commands.spawn((
            SpriteBundle {
                texture,
                transform,
                ..default()
            },
            TextureAtlas {
                layout: texture_atlas_layout,
                index: 0,
            },
            Curve {
                path: vec![ev.start, ev.target],
                time: Stopwatch::new(),
                speed: 256.0,
            },
            Projectile,
        ));
    }
}

/// Despan projectiles that are no longer moving
pub(super) fn cleanup_projectiles(
    mut commands: Commands,
    query: Query<Entity, (With<Projectile>, Without<Curve>)>,
) {
    for entity in &query {
        commands.entity(entity).despawn_recursive();
    }
}

pub(super) fn process_curves(
    mut commands: Commands,
    time: Res<Time>,
    mut gizmos: Gizmos,
    mut query: Query<(Entity, &mut Transform, &mut Curve)>,
) {
    for (entity, mut transform, mut curve) in &mut query {
        gizmos.linestrip_2d(curve.path.clone(), Color::WHITE);
        curve.time.tick(time.delta());
        let pos = curve.cur_pos();
        transform.translation = pos.extend(transform.translation.z);

        if curve.is_finished() {
            commands.entity(entity).remove::<Curve>();
        }
    }
}

pub(super) fn game_over(mut next_state: ResMut<NextState<PlayState>>, sim: Res<SimState>) {
    if sim.is_terminal() {
        warn!("game over");
        next_state.set(PlayState::Terminal);
    }
}

pub(super) fn ai(sim: Res<SimState>, mut ev_action: EventWriter<ActionEvent>) {
    if matches!(sim.cur_team(), Team::NPCs(_) | Team::Players(_)) {
        debug!("finding best move for: {:?}", sim.cur_char());
        let action = find_best_move(sim.clone()).expect("failed to find a best move");
        ev_action.send(ActionEvent { action });
    }
}

pub(super) fn healthbars(mut gizmos: Gizmos, query: Query<(&Transform, &Health)>) {
    const BAR_WIDTH: f32 = TILE_SIZE as f32 * 0.8; // 80% of grid item for health
    for (transform, health) in &query {
        let left = transform.translation.x - BAR_WIDTH / 2.0;
        let bar_fill_frac = health.cur as f32 / health.max as f32;
        let right = left + BAR_WIDTH * bar_fill_frac;
        // translation.y is the middle of the grid. We want to have the health bar slightly above the top of the grid
        // so we divide by slightly less than 2 to place it
        let y = transform.translation.y + TILE_SIZE as f32 / 2.0;
        gizmos.line_2d(vec2(left, y), vec2(right, y), HEALTH_BAR_COLOR);
    }
}

#[derive(Component)]
pub(super) struct Health {
    cur: u8,
    max: u8,
}

impl CharacterSprite {
    pub fn idle(&self) -> String {
        match self {
            CharacterSprite::Skeleton => format!(
                "{}{}",
                "pixel-crawler/Enemy/Skeleton Crew/Skeleton - Base", IDLE_LOCATION
            ),
            CharacterSprite::Knight => {
                format!("{}{}", "pixel-crawler/Heroes/Knight", IDLE_LOCATION)
            }
            CharacterSprite::Orc => {
                format!("{}{}", "pixel-crawler/Enemy/Orc Crew/Orc", IDLE_LOCATION)
            }
            CharacterSprite::Wizard => {
                format!("{}{}", "pixel-crawler/Heroes/Wizard", IDLE_LOCATION)
            }
        }
    }
}
