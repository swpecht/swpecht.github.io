use std::fs;

use bevy::{math::vec3, prelude::*};
use simulation::{gamestate::SimId, ModelSprite};

use crate::sim_wrapper::SimIdComponent;

use super::{animation::AnimationConfig, Health, CHAR_LAYER};

#[derive(Event)]
pub struct CharacterSpawnEvent {
    pub(super) id: SimId,
    pub(super) sprite: ModelSprite,
    pub(super) animation: CharacterAnimation,
    pub(super) loc: Vec2,
    pub(super) health: Health,
}

#[derive(Event)]
pub struct CharacterAnimationUpdateEvent {}

#[derive(Component)]
pub enum CharacterAnimation {
    IDLE,
    RUN,
}

impl CharacterAnimation {
    fn location(&self) -> &str {
        match self {
            CharacterAnimation::IDLE => "/Idle/Idle",
            CharacterAnimation::RUN => "/Run/Run",
        }
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
