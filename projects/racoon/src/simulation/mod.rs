use anyhow::Context;
use bevy::prelude::*;

pub const SIMULATION_WIDTH: usize = 100;
pub const SIMULATION_HEIGHT: usize = 100;

/// Handles the underlying discrete simulation for the game
///
/// This is the true state of the world and progresses as the actions are slowly applied over time, the actions determine the delta for the state
///
/// Everything else is a lerp of this system to fake real-time movement
/// We need to keep track of progress to the next simulation tick
pub struct SimulationPlugin {}

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        let mut gs = SimulationState::default();
        gs.spawn_entity(
            Entity::RangedUnit {
                team: Team::Computer,
                health: 100,
            },
            Coordinates { x: 50, y: 50 },
        )
        .unwrap();

        gs.spawn_entity(
            Entity::RangedUnit {
                team: Team::Player,
                health: 100,
            },
            Coordinates { x: 50, y: 51 },
        )
        .unwrap();

        gs.spawn_entity(Entity::Wall, Coordinates { x: 51, y: 51 })
            .unwrap();

        app.insert_resource(gs)
            .add_systems(Startup, setup)
            .add_systems(Update, (tick_simulation_system, display_sim_state));
    }
}

/// The `SimulationState` is the GameState from card_platypus.
///
/// Every "tick" each player must choose an action for every entity they control. The choosen actions are not revealed to the other player
/// until the tick is finished, Once the tick is finished, all actions are executed at once and the process is repeated. The order of choosing actions
/// is not particularly important as long as it it deterministic to try and minimize the number of istates.
///
/// The phyics or unit system, then take the old simulaiton state and the new simulation state and lerp together the results
///
/// Need to skip over non-controlled entities
#[derive(Resource)]
struct SimulationState {
    entities: Vec<(Coordinates, Entity)>,
    action_queue: Vec<Action>,
}

impl Default for SimulationState {
    fn default() -> Self {
        let gs = Self {
            entities: Vec::new(),
            action_queue: Vec::new(),
        };

        gs
    }
}

impl SimulationState {
    pub fn spawn_entity(&mut self, entity: Entity, coords: Coordinates) -> anyhow::Result<()> {
        assert!(
            self.action_queue.is_empty(),
            "cannot spawn entites mid turn"
        );

        self.entities.push((coords, entity));

        Ok(())
    }

    /// Todo: figure out how we want this to work -- what will easily allow us to do cfr?
    /// do we somehow iterate through our units -- maybe from top left to bottom right?
    /// then we queue up the actions
    ///
    /// First we select attack action, then the move action? -- how do we track this istate? -- show the set of what other units are going to do
    pub fn legal_actions(&self) -> Vec<Action> {
        // todo: add support for wall being the first entity

        let cur_entity = self.cur_entity();
        match cur_entity {
            Entity::RangedUnit { team: _, health: _ } => self.legal_actions_ranged(),
            Entity::Wall => panic!("should never be a walls turn"),
        }
    }

    fn legal_actions_ranged(&self) -> Vec<Action> {
        let mut actions = Vec::new();
        use Action::*;
        actions.push(Pass);

        let coords = self.cur_entity_coords();
        let x_range = [-1, 0, 1];
        let y_range = [-1, 0, 1];

        for x_d in x_range {
            for y_d in y_range {
                if x_d == 0 && y_d == 0 {
                    continue;
                }

                actions.push(Move {
                    x: (coords.x as isize + x_d) as usize,
                    y: (coords.y as isize + y_d) as usize,
                })
            }
        }

        // todo implement attack actions

        actions.sort();
        actions
    }

    /// Queue up an action to be executed once all actions have been queued
    pub fn apply_action(&mut self, a: Action) {
        self.action_queue.push(a);

        // add in automatic passing for non-controllable entities
        while self.action_queue.len() < self.num_entities()
            && matches!(self.cur_entity(), Entity::Wall)
        {
            self.action_queue.push(Action::Pass);
        }

        // if we're at the end of the queue, process all actions
        if self.num_entities() == self.action_queue.len() {
            self.process_actions()
        }
    }

    pub fn cur_team(&self) -> Team {
        match self.cur_entity() {
            Entity::RangedUnit { team, health: _ } => *team,
            Entity::Wall => panic!("should never be a walls turn to go"),
        }
    }

    /// Apply all actions and clear the action queue
    ///
    /// Actions are applied in the following order:
    /// *...
    fn process_actions(&mut self) {
        assert_eq!(self.action_queue.len(), self.entities.len());

        // check for legal actions
        self.entities
            .iter_mut()
            .zip(&self.action_queue)
            .for_each(|((coords, entity), action)| {
                use Action::*;
                match (entity, action) {
                    (_, Pass) => {}
                    (_, Move { x, y }) => *coords = Coordinates { x: *x, y: *y },
                    _ => panic!("unsupported entity, action combination",),
                }
            });

        self.action_queue.clear();
    }

    pub fn undo() {
        todo!()
    }

    fn cur_entity(&self) -> &Entity {
        self.get_entity(self.action_queue.len()).unwrap()
    }

    fn cur_entity_coords(&self) -> Coordinates {
        let n = self.action_queue.len();
        self.entities[n].0
    }

    fn get_entity(&self, loc: usize) -> Option<&Entity> {
        self.entities.get(loc).map(|(_, x)| x)
    }

    fn num_entities(&self) -> usize {
        self.entities.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entity {
    RangedUnit { team: Team, health: usize },
    Wall,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Action {
    Attack { x: usize, y: usize },
    Move { x: usize, y: usize },
    Pass,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Team {
    Player,
    Computer,
}

/// Location in the simulated world
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Coordinates {
    pub x: usize,
    pub y: usize,
}

fn setup() {}

fn tick_simulation_system() {}

fn display_sim_state(mut gizmos: Gizmos, gs: Res<SimulationState>) {
    for (coords, unit) in gs.entities.iter() {
        // info!("found unit: {:?}", unit);
        let position: Vec2 = coords.into();
        match unit {
            Entity::RangedUnit { team, health: _ } => {
                gizmos.circle_2d(
                    position,
                    25.,
                    match team {
                        Team::Player => Color::GREEN,
                        Team::Computer => Color::RED,
                    },
                );
            }
            Entity::Wall => gizmos.rect_2d(position, 0., Vec2 { x: 50., y: 50. }, Color::WHITE),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::simulation::Action;

    use super::{Coordinates, Entity, SimulationState, Team};

    #[test]
    fn test_entity_coords() {
        let mut gs = SimulationState::default();
        gs.spawn_entity(
            Entity::RangedUnit {
                team: Team::Computer,
                health: 100,
            },
            Coordinates { x: 50, y: 50 },
        )
        .unwrap();

        gs.spawn_entity(
            Entity::RangedUnit {
                team: Team::Player,
                health: 100,
            },
            Coordinates { x: 90, y: 90 },
        )
        .unwrap();

        gs.spawn_entity(Entity::Wall, Coordinates { x: 51, y: 51 })
            .unwrap();

        assert_eq!(gs.cur_entity_coords(), Coordinates { x: 50, y: 50 });
        assert!(matches!(
            gs.cur_entity(),
            Entity::RangedUnit {
                team: Team::Computer,
                health: _
            }
        ));

        use Action::*;
        gs.apply_action(Move { x: 49, y: 50 });
        assert_eq!(gs.cur_entity_coords(), Coordinates { x: 90, y: 90 });
        gs.apply_action(Move { x: 91, y: 90 });

        // the entity moved
        assert_eq!(gs.cur_entity_coords(), Coordinates { x: 49, y: 50 });
    }
}
