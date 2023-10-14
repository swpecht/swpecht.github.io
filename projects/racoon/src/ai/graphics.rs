use bevy::prelude::*;

use crate::units::GoalPos;

use super::ACHIEVE_GOAL_DELTA;

// Helper function to visualize AI goals
pub(super) fn render_ai_goals(mut gizmos: Gizmos, query: Query<&GoalPos>) {
    for pos in &query {
        gizmos.circle_2d(**pos, ACHIEVE_GOAL_DELTA, Color::GREEN);
    }
}

