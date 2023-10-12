use bevy::prelude::*;
use rand::{thread_rng, Rng};

use crate::{physics::Position, units::GoalPos};

use self::graphics::render_ai_goals;

/// How close an entity needs to be to a goal before it is considered on it
const ACHIEVE_GOAL_DELTA: f32 = 10.0;

mod graphics;

pub struct AIPlugin {}

impl Plugin for AIPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (set_goal_system, remove_goal_system))
            .add_systems(Update, render_ai_goals);
    }
}

fn setup() {}

/// Add a goal position to every ai controlled entity
fn set_goal_system(
    mut commands: Commands,
    query: Query<Entity, (With<AIControlled>, Without<GoalPos>)>,
) {
    let rng = &mut thread_rng();
    for entity in &query {
        commands.entity(entity).insert(GoalPos(Vec2 {
            x: rng.gen_range(-500.0..500.),
            y: rng.gen_range(-500.0..500.),
        }));
    }
}

fn remove_goal_system(
    mut commands: Commands,
    query: Query<(Entity, &GoalPos, &Position), With<AIControlled>>,
) {
    for (entity, gpos, pos) in &query {
        if (**gpos - **pos).length() < ACHIEVE_GOAL_DELTA {
            commands.entity(entity).remove::<GoalPos>();
        }
    }
}

#[derive(Component)]
pub struct AIControlled {}
