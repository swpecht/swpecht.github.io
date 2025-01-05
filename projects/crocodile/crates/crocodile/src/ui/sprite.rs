use std::fs;

use bevy::{
    math::{vec2, vec3},
    prelude::*,
    render::camera::ScalingMode,
    time::Stopwatch,
};
use simulation::gamestate::{ActionResult, SimCoords, SimId, SimState};

use crate::{
    ui::{ActionEvent, PROJECTILE_LAYER},
    PlayState,
};

use super::{
    character::{CharacterAnimation, CharacterSpawnEvent},
    to_world, TILE_LAYER,
};

pub const TILE_SIZE: usize = 32;
const GRID_WIDTH: usize = 20;
const GRID_HEIGHT: usize = 20;

const HEALTH_BAR_COLOR: Color = Color::srgb(1.0, 0.0, 0.0);

#[derive(Component, Clone)]
pub struct Curve {
    path: Vec<Vec2>,
    time: Stopwatch,
    speed: f32,
}

#[derive(Event, Debug)]
pub(super) struct SpawnProjectileEvent {
    from: Vec2,
    to: Vec2,
}

#[derive(Component)]
pub(super) struct Projectile;

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
        *self.path.last().unwrap()
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
    commands.spawn((
        Camera2d {},
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: (GRID_HEIGHT * TILE_SIZE + TILE_SIZE) as f32,
            },
            ..OrthographicProjection::default_2d()
        }),
        Transform::from_xyz(
            ((GRID_WIDTH + 1) * TILE_SIZE / 2) as f32,
            ((GRID_HEIGHT + 1) * TILE_SIZE / 2) as f32,
            0.0,
        ),
        MainCamera,
    ));
}

pub(super) fn sync_sim(
    mut commands: Commands,
    sim: Res<SimState>,
    spawned_entities: Query<Entity, With<SimId>>,
) {
    // delete everything that's already spawned
    for entity in &spawned_entities {
        commands.entity(entity).despawn_recursive();
    }

    // respawn everything
    for (id, loc, sprite) in sim
        .sprites()
        .into_iter()
        .map(|(id, l, c)| (id, to_world(&l), c))
    {
        // TODO: add support for changing location of things if they're already spawned
        commands.send_event(CharacterSpawnEvent {
            id,
            sprite,
            animation: CharacterAnimation::IDLE,
            loc,
            health: Health {
                cur: sim.health(&id).unwrap(),
                max: sim.max_health(&id).unwrap(),
            },
        });
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
                Sprite::from_atlas_image(
                    texture.clone(),
                    TextureAtlas {
                        layout: texture_atlas_layout.clone(),
                        index: 5,
                    },
                ),
                Transform::from_translation(vec3(
                    (r * TILE_SIZE) as f32,
                    (c * TILE_SIZE) as f32,
                    TILE_LAYER,
                )),
            ));
        }
    }
}

/// Translate action events into the proper display within the game visualization
pub(super) fn action_system(
    mut commands: Commands,
    mut ev_action: EventReader<ActionEvent>,
    query: Query<(Entity, &SimId, &Transform)>,
    mut sim: ResMut<SimState>,
    mut next_state: ResMut<NextState<PlayState>>,
) {
    for ev in ev_action.read() {
        debug!("action event received: {:?}", ev);
        sim.apply(ev.action);

        for ar in sim.diff() {
            match ar {
                ActionResult::Move {
                    id,
                    from: _,
                    to: end,
                } => handle_move(&mut commands, end, &query, id),
                // Reset the ui
                ActionResult::EndTurn => next_state.set(PlayState::Processing),
                ActionResult::RemoveModel { id: _id } => {
                    next_state.set(PlayState::Processing);
                }
                _ => {} // no ui impact for most actions
            }
        }
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
                path: vec![start, to_world(&target)],
                time: Stopwatch::new(),
                speed: 64.0,
            };
            commands
                .entity(e)
                .insert((curve.clone(), CharacterAnimation::RUN));
        });
}

fn _handle_melee(
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
                path: vec![start, to_world(&target).lerp(start, 0.5), start],
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
        let metadata = json::parse(
            &fs::read_to_string("assets/pixel-crawler/Weapons/Wood/Wood.json").unwrap(),
        )
        .unwrap();
        let w = metadata["meta"]["size"]["w"].as_u32().unwrap();
        let h = metadata["meta"]["size"]["h"].as_u32().unwrap();
        let mut layout = TextureAtlasLayout::new_empty(UVec2::new(w, h));

        layout.add_texture(URect::from_corners(UVec2::new(32, 0), UVec2::new(48, 16)));
        let texture_atlas_layout = texture_atlas_layouts.add(layout);
        let angle = (ev.to - ev.from).angle_to(ev.from);
        let mut transform = Transform::from_xyz(ev.from.x, ev.from.y, PROJECTILE_LAYER);
        transform.rotation = Quat::from_rotation_z(angle);

        commands.spawn((
            Sprite::from_atlas_image(
                texture,
                TextureAtlas {
                    layout: texture_atlas_layout,
                    index: 0,
                },
            ),
            transform,
            Curve {
                path: vec![ev.from, ev.to],
                time: Stopwatch::new(),
                speed: 125.0, // 256.0,
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
    mut query: Query<(Entity, &mut Transform, &mut Curve)>,
) {
    for (entity, mut transform, mut curve) in &mut query {
        curve.time.tick(time.delta());
        let pos = curve.cur_pos();
        transform.translation = pos.extend(transform.translation.z);

        if curve.is_finished() {
            commands.entity(entity).remove::<Curve>();
        }
    }
}

pub(super) fn _paint_curves(mut gizmos: Gizmos, query: Query<&Curve>) {
    for curve in &query {
        gizmos.linestrip_2d(curve.path.clone(), Color::WHITE);
    }
}

pub(super) fn game_over(mut _next_state: ResMut<NextState<PlayState>>, _sim: Res<SimState>) {
    // if sim.is_terminal() {
    //     warn!("game over");
    //     next_state.set(PlayState::Terminal);
    // }
}

pub(super) fn ai(_sim: Res<SimState>, mut _ev_action: EventWriter<ActionEvent>) {
    // disable the ai for now
    // if matches!(sim.cur_team(), Team::NPCs | Team::Players) {
    //     debug!("finding best move for: {:?}", sim.cur_char());
    //     let action = find_best_move(sim.clone()).expect("failed to find a best move");
    //     ev_action.send(ActionEvent { action });
    // }
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

#[derive(Component, Clone)]
pub(super) struct Health {
    cur: u8,
    max: u8,
}
