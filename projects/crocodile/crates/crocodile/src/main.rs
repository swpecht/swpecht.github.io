use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    DefaultPlugins,
};
use crocodile::{gamestate::SimState, ui::UIPlugin, StatePlugin};

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
                    filter: "wgpu=error,bevy_render=info,bevy_ecs=info,naga=error".to_string(),
                    ..default()
                }),
        ) // prevents blurry sprites
        .add_plugins((StatePlugin, UIPlugin))
        .init_resource::<SimState>()
        .run();
}
