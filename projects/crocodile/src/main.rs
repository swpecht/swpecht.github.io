use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    render::color::Color,
    DefaultPlugins,
};
use crocodile::{gamestate::SimState, sprite::SpritePlugin, ui::UIPlugin, PlayState, StatePlugin};

const BACKGROUND_COLOR: Color = Color::rgb(0.9, 0.9, 0.9);

pub enum TransitionState {
    Waiting,    // waiting on an action
    Processing, // processing an action
}

fn main() {
    bevy::app::App::new()
        .add_plugins(
            DefaultPlugins
                .set(ImagePlugin::default_nearest())
                .set(LogPlugin {
                    level: Level::DEBUG,
                    filter: "wgpu=error,bevy_render=info,bevy_ecs=trace".to_string(),
                    update_subscriber: None,
                }),
        ) // prevents blurry sprites
        .add_plugins((StatePlugin, UIPlugin, SpritePlugin))
        .add_systems(Update, bevy::window::close_on_esc)
        .init_resource::<SimState>()
        .run();
}
