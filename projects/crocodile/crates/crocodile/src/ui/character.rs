use std::fs;

use crate::ui::sprite::Curve;
use crate::{
    sim_wrapper::{SimIdComponent, SimStateResource},
    ui::to_world,
    CHAR_LAYER, TILE_SIZE, UI_LAYER,
};
use bevy::color::palettes::css::RED;
use bevy::math::vec2;
use bevy::time::Stopwatch;
use bevy::{color::palettes::css::BLACK, math::vec3, prelude::*};
use simulation::{
    gamestate::{ActionResult, ModelId},
    ModelSprite,
};

use super::{animation::AnimationConfig, Health};

#[derive(Event)]
pub struct CharacterSpawnEvent {
    pub(super) id: ModelId,
    pub(super) sprite: ModelSprite,
    pub(super) animation: CharacterAnimation,
    pub(super) loc: Vec2,
    pub(super) health: Health,
}

#[derive(Event)]
pub struct WeaponResolutionEvent {
    pub id: ModelId,
    pub result: ActionResult,
}

#[derive(Event)]
pub struct CharacterAnimationUpdateEvent {}

#[derive(Component)]
pub enum CharacterAnimation {
    IDLE,
    RUN,
}

#[derive(Component)]
pub(super) struct WeaponResolutionText;

impl CharacterAnimation {
    fn location(&self) -> &str {
        match self {
            CharacterAnimation::IDLE => "/Idle/Idle",
            CharacterAnimation::RUN => "/Run/Run",
        }
    }
}

pub(super) fn weapon_resolution(
    mut commands: Commands,
    mut event_reader: EventReader<WeaponResolutionEvent>,
    sim: Res<SimStateResource>,
) {
    for event in event_reader.read() {
        let loc = to_world(&sim.0.get_loc(event.id).unwrap());

        let (text, color) = match event.result {
            ActionResult::Miss { id: _ } => (Text2d::new("miss"), TextColor(Color::Srgba(RED))),
            ActionResult::Hit { id: _ } => (Text2d::new("hit"), TextColor(Color::Srgba(BLACK))),
            _ => panic!("invalid action result for weapon resolution"),
        };

        let start = vec2(loc.x, loc.y + (TILE_SIZE / 2) as f32);
        let target = start + vec2(0., TILE_SIZE as f32);
        commands.spawn((
            WeaponResolutionText,
            text,
            color,
            TextFont {
                font_size: 20.0,
                ..default()
            },
            Transform {
                translation: vec3(start.x, start.y, UI_LAYER),
                ..default()
            },
            Curve {
                path: vec![start, target],
                time: Stopwatch::new(),
                speed: 64.0,
            },
        ));
    }
}

/// Despawn projectiles that are no longer moving
pub(super) fn cleanup_resolution_text(
    mut commands: Commands,
    query: Query<Entity, (With<WeaponResolutionText>, Without<Curve>)>,
) {
    for entity in &query {
        commands.entity(entity).despawn_recursive();
    }
}

pub(super) fn spawn_character(
    mut commands: Commands,
    mut event_reader: EventReader<CharacterSpawnEvent>,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    for event in event_reader.read() {
        debug!("Spawning character");

        let base_path = format!("{}{}", event.sprite.asset_loc(), event.animation.location());
        let texture = asset_server.load(format!("{}{}", base_path, ".png"));

        let meta_file = format!("assets/{}{}", base_path, ".json");
        let metadata = json::parse(
            &fs::read_to_string(&meta_file)
                .unwrap_or_else(|x| panic!("failed to load: {}: {}", meta_file, x)),
        )
        .unwrap();
        let num_frames = metadata["frames"].len() as u32;
        let w = metadata["meta"]["size"]["w"].as_u32().unwrap() / num_frames;

        let layout = TextureAtlasLayout::from_grid(UVec2::new(w, w), num_frames, 1, None, None);
        let texture_atlas_layout = texture_atlas_layouts.add(layout);

        commands.spawn((
            SimIdComponent(event.id),
            Sprite::from_atlas_image(
                texture,
                TextureAtlas {
                    layout: texture_atlas_layout,
                    index: 0,
                },
            ),
            AnimationConfig {
                first_animation_frame: 0,
                last_animation_frame: num_frames as usize - 1,
                frame_timer: Timer::from_seconds(0.1, TimerMode::Repeating),
            },
            Transform {
                translation: vec3(event.loc.x, event.loc.y, CHAR_LAYER),
                scale: vec3(32.0 / w as f32, 32.0 / w as f32, 1.0),
                ..default()
            },
            event.health.clone(),
        ));
    }
}
