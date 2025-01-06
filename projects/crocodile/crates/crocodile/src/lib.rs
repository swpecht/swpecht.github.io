#![feature(test)]
#![feature(let_chains)]
#![feature(get_many_mut)]

use bevy::prelude::*;

pub mod ui;

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
