use std::fs;

use bevy::{math::vec3, prelude::*};

use super::CharacterSprite;

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

pub(super) fn update_animation(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
    query: Query<(Entity, &CharacterSprite, &Animation, &Transform), Changed<Animation>>,
) {
    for (entity, character, animation, transform) in &query {
        let mut animation_bundle = load_animation(
            &asset_server,
            &mut texture_atlas_layouts,
            character,
            animation,
        );
        animation_bundle.sb.transform = *transform;
        commands.entity(entity).insert(animation_bundle);
    }
}

#[derive(Component)]
pub(super) struct AnimationIndices {
    pub first: usize,
    pub last: usize,
}

#[derive(Component, Deref, DerefMut)]
pub(super) struct AnimationTimer(Timer);

#[derive(Component)]
pub enum Animation {
    IDLE,
    RUN,
}

impl Animation {
    fn location(&self) -> &str {
        match self {
            Animation::IDLE => "/Idle/Idle",
            Animation::RUN => "/Run/Run",
        }
    }
}

impl CharacterSprite {
    fn asset_loc(&self) -> &str {
        match self {
            CharacterSprite::Skeleton => "pixel-crawler/Enemy/Skeleton Crew/Skeleton - Base",
            CharacterSprite::Knight => "pixel-crawler/Heroes/Knight",
            CharacterSprite::Orc => "pixel-crawler/Enemy/Orc Crew/Orc",
            CharacterSprite::Wizard => "pixel-crawler/Heroes/Wizard",
        }
    }
}
#[derive(Bundle)]
pub struct AnimationBundle {
    pub sb: SpriteBundle,
    texture_atlas: TextureAtlas,
    animation_indices: AnimationIndices,
    animation_timer: AnimationTimer,
    character_sprite: CharacterSprite,
}

pub(super) fn load_animation(
    asset_server: &Res<AssetServer>,
    texture_atlas_layouts: &mut ResMut<Assets<TextureAtlasLayout>>,
    character: &CharacterSprite,
    animation: &Animation,
) -> AnimationBundle {
    let base_path = format!("{}{}", character.asset_loc(), animation.location());

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
    // Use only the subset of sprites in the sheet that make up the run animation
    let animation_indices = AnimationIndices {
        first: 0,
        last: num_frames as usize - 1,
    };

    AnimationBundle {
        sb: SpriteBundle {
            texture,
            transform: Transform::from_scale(vec3(32.0 / w as f32, 32.0 / w as f32, 1.0)),
            ..default()
        },
        texture_atlas: TextureAtlas {
            layout: texture_atlas_layout,
            index: animation_indices.first,
        },
        animation_indices,
        animation_timer: AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
        character_sprite: *character,
    }
}
