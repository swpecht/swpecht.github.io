use bevy::prelude::{Component, Resource};
use simulation::gamestate::{ModelId, SimState};

#[derive(Component)]
pub struct SimIdComponent(pub ModelId);

#[derive(Resource, Default)]
pub struct SimStateResource(pub SimState);

impl From<SimIdComponent> for ModelId {
    fn from(value: SimIdComponent) -> Self {
        value.0
    }
}

impl From<SimStateResource> for SimState {
    fn from(value: SimStateResource) -> Self {
        value.0
    }
}
