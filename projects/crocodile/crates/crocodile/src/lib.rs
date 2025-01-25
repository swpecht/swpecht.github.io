#![feature(test)]
#![feature(let_chains)]
#![feature(get_many_mut)]

use bevy::prelude::*;

pub mod game_area;
pub mod sim_wrapper;
pub mod ui;

pub const TILE_LAYER: f32 = 0.0;
pub const CHAR_LAYER: f32 = 1.0;
const PROJECTILE_LAYER: f32 = 2.0;
const UI_LAYER: f32 = 3.0;

const NORMAL_BUTTON: Color = Color::srgb(0.15, 0.15, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.25, 0.25, 0.25);
const PRESSED_BUTTON: Color = Color::srgb(0.35, 0.75, 0.35);
const VALID_MOVE: Color = Color::srgba(0.0, 0.5, 0.5, 0.5);
const INCOHERENT_UNIT: Color = Color::srgba(0.7, 0.0, 0.0, 0.5);

pub const TILE_SIZE: usize = 32;
const GRID_WIDTH: usize = 20;
const GRID_HEIGHT: usize = 20;

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            transition_to_waiting.run_if(in_state(PlayState::Setup)),
        )
        .add_systems(Update, monitor_processing)
        .init_state::<PlayState>()
        .enable_state_scoped_entities::<PlayState>();
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum PlayState {
    #[default]
    Setup,
    Waiting,
    Processing,
    Terminal,
}

/// Moves back into the Waiting state once all processing has finished
fn monitor_processing(
    state: Res<State<PlayState>>,
    mut next_state: ResMut<NextState<PlayState>>,
    query: Query<Entity, With<ui::sprite::Curve>>,
) {
    use PlayState::*;
    match (state.get(), &query.iter().len()) {
        (Processing, 0) => {
            debug!("changing to waiting state");
            next_state.set(Waiting)
        }
        (Processing, _) => {}
        (Waiting, 0) => {}
        (Waiting, _) => {
            debug!("changing to processing state");
            next_state.set(Processing)
        }
        (_, _) => {}
    }
}

fn transition_to_waiting(mut app_state: ResMut<NextState<PlayState>>) {
    app_state.set(PlayState::Waiting);
}
