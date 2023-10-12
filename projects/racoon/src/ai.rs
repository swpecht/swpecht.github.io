use bevy::prelude::*;

pub struct AIPlugin {}

impl Plugin for AIPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, navigation_system);
    }
}

fn setup() {}

fn navigation_system() {}
