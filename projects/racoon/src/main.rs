use bevy::{ecs::query, prelude::*};
use racoon::{
    input::{CursorPlugin, MouseCoords},
    render::WorldRenderPlugin,
    units::{EnemyBundle, EnemySpawn, Position, Spawner},
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((CursorPlugin {}, WorldRenderPlugin {}))
        .add_systems(Startup, setup)
        .add_systems(Update, (mouse_click_system, spawner_system))
        .add_systems(FixedUpdate, move_unit_system)
        .run();
}

fn mouse_click_system(mouse_cords: Res<MouseCoords>, mouse_button_input: Res<Input<MouseButton>>) {
    if mouse_button_input.just_released(MouseButton::Left) {
        info!(
            "left mouse just released: {}, {}",
            mouse_cords.loc().x,
            mouse_cords.loc().y
        );
    }
}

fn setup(mut commands: Commands) {
    commands.spawn(EnemySpawn::new(Vec2 { x: -153., y: 76. }));
    commands.spawn(EnemySpawn::new(Vec2 { x: 183., y: 76. }));
}

fn move_unit_system(mut query: Query<&mut Position, Without<Spawner>>) {
    for mut pos in &mut query {
        pos.loc.y -= 1.;
    }
}

fn spawner_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(&mut Spawner, &Position)>,
) {
    for (mut s, pos) in &mut query {
        if s.timer.tick(time.delta()).just_finished() {
            commands.spawn(EnemyBundle::new(*pos.loc() + s.range));
        }
    }
}
