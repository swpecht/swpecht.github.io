use bevy::{
    log::{Level, LogPlugin},
    prelude::*,
    render::color::Color,
    DefaultPlugins,
};
use crocodile::{gamestate::SimState, sprite::SpritePlugin, ui::UIPlugin};

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
        .add_plugins((UIPlugin, SpritePlugin))
        // .insert_resource(Scoreboard { score: 0 })
        // .insert_resource(ClearColor(BACKGROUND_COLOR))
        // .add_event::<CollisionEvent>()
        // .add_systems(Startup, (setup_camera))
        // // Add our gameplay simulation systems to the fixed timestep schedule
        // // which runs at 64 Hz by default
        // .add_systems(Update)
        // .add_systems(
        //     FixedUpdate,
        //     (
        //         apply_velocity,
        //         move_paddle,
        //         check_for_collisions,
        //         play_collision_sound,
        //     )
        //         // `chain`ing systems together runs them in order
        //         .chain(),
        // )
        .add_systems(Update, bevy::window::close_on_esc)
        .init_resource::<SimState>()
        .run();
}
