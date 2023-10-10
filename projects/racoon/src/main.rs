use bevy::prelude::*;
use racoon::{
    input::{CursorPlugin, MouseCoords},
    render::WorldRenderPlugin,
    units::{EnemyBundle, Position},
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((CursorPlugin {}, WorldRenderPlugin {}))
        .add_systems(Update, mouse_click_system)
        .add_systems(FixedUpdate, move_unit_system)
        .run();
}

fn mouse_click_system(
    mouse_cords: Res<MouseCoords>,
    mut commands: Commands,
    mouse_button_input: Res<Input<MouseButton>>,
) {
    if mouse_button_input.pressed(MouseButton::Left) {
        info!("left mouse currently pressed");
    }

    if mouse_button_input.just_pressed(MouseButton::Left) {
        info!("left mouse just pressed");
    }

    if mouse_button_input.just_released(MouseButton::Left) {
        info!("left mouse just released");
        commands.spawn(EnemyBundle::new(*mouse_cords.loc()));
    }
}

fn move_unit_system(mut query: Query<&mut Position>) {
    for mut pos in &mut query {
        pos.loc.y += 1.;
    }
}
