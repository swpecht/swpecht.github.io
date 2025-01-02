use bevy::prelude::*;

pub(super) fn animate_sprite(
    time: Res<Time>,
    mut query: Query<(&mut AnimationConfig, &mut Sprite)>,
) {
    for (mut config, mut sprite) in &mut query {
        config.frame_timer.tick(time.delta());
        if config.frame_timer.just_finished() {
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index = if atlas.index == config.last_animation_frame {
                    config.first_animation_frame
                } else {
                    atlas.index + 1
                };
            }
        }
    }
}

#[derive(Component)]
pub struct AnimationConfig {
    pub first_animation_frame: usize,
    pub last_animation_frame: usize,
    pub frame_timer: Timer,
}
