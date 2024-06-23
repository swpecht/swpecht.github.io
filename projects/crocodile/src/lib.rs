use bevy::prelude::*;
use sprite::Curve;

pub mod gamestate;
pub mod sprite;
pub mod ui;

pub struct StatePlugin;

impl Plugin for StatePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, monitor_processing)
            .init_state::<PlayState>();
    }
}

#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum PlayState {
    #[default]
    Waiting,
    Processing,
}

/// Moves back into the Waiting state once all processing has finished
fn monitor_processing(
    state: Res<State<PlayState>>,
    mut next_state: ResMut<NextState<PlayState>>,
    query: Query<Entity, With<Curve>>,
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
    }
}
