use bevy::prelude::*;
use racoon::{
    ai::{AIControlled, AIPlugin},
    graphics::{AnimatedSpriteBundle, GraphicsPlugin},
    input::{CursorPlugin, MouseCoords},
    physics::{PhyscisPlugin, Position},
    simulation::SimulationPlugin,
    units::{EnemyBundle, EnemySpawnBundle, SpawnerTimer, UnitsPlugin},
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((
            CursorPlugin {},
            GraphicsPlugin {},
            PhyscisPlugin {},
            AIPlugin {},
            UnitsPlugin {},
            SimulationPlugin {},
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, (mouse_click_system, spawner_system))
        // .add_systems(Update, grid_system)
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
    commands.spawn(EnemySpawnBundle::new(Vec2 { x: -153., y: 76. }));
    commands.spawn(EnemySpawnBundle::new(Vec2 { x: 183., y: 76. }));
}

fn spawner_system(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(&mut SpawnerTimer, &Position)>,
    asset_server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
) {
    for (mut s, pos) in &mut query {
        if s.tick(time.delta()).just_finished() {
            let spawn_pos = **pos + Vec2 { x: 20., y: 20. };
            let texture_handle = asset_server.load("corgi.png");
            let texture_atlas =
                TextureAtlas::from_grid(texture_handle, Vec2::new(32.0, 32.0), 2, 2, None, None);
            let texture_atlas_handle = texture_atlases.add(texture_atlas);
            let sprite = AnimatedSpriteBundle::new(texture_atlas_handle, spawn_pos);
            commands.spawn((EnemyBundle::new(spawn_pos, sprite), AIControlled {}));
        }
    }
}
