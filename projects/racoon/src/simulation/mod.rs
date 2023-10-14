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
        app.insert_resource(SimulationState::default())
            .add_systems(Startup, setup)
            .add_systems(Update, (tick_simulation_system, display_sim_state));
    }
}

#[derive(Resource)]
struct SimulationState {
    entities: Vec<Vec<Vec<Entity>>>,
}

impl Default for SimulationState {
    fn default() -> Self {
        let mut entities = Vec::with_capacity(SIMULATION_WIDTH);
        for x in 0..SIMULATION_WIDTH {
            entities.push(Vec::with_capacity(SIMULATION_HEIGHT));
            for _ in 0..SIMULATION_HEIGHT {
                entities[x].push(Vec::new())
            }
        }

        entities[50][50].push(Entity::RangedUnit {
            team: Team::Computer,
            health: 100,
        });

        entities[50][51].push(Entity::RangedUnit {
            team: Team::Player,
            health: 100,
        });

        Self { entities }
    }
}

impl SimulationState {
    /// Todo: figure out how we want this to work -- what will easily allow us to do cfr?
    /// do we somehow iterate through our units -- maybe from top left to bottom right?
    /// then we queue up the actions
    ///
    /// First we select attack action, then the move action? -- how do we track this istate? -- show the set of what other units are going to do
    pub fn legal_actions() -> Vec<Action> {
        todo!()
    }
}

#[derive(Debug)]
enum Entity {
    RangedUnit { team: Team, health: usize },
    Wall,
}

enum Action {
    Attack { x: usize, y: usize },
    Move { x: usize, y: usize },
    Pass,
}

#[derive(Debug)]
enum Team {
    Player,
    Computer,
}

/// Location in the simulated world
pub struct Coordinates {
    pub x: usize,
    pub y: usize,
}

fn setup() {}

fn tick_simulation_system() {}

fn display_sim_state(mut gizmos: Gizmos, gs: Res<SimulationState>) {
    for (x, row) in gs.entities.iter().enumerate() {
        for (y, units) in row.iter().enumerate() {
            for unit in units {
                // info!("found unit: {:?}", unit);

                match unit {
                    Entity::RangedUnit { team, health: _ } => {
                        gizmos.circle_2d(
                            Coordinates { x, y }.into(),
                            20.,
                            match team {
                                Team::Player => Color::GREEN,
                                Team::Computer => Color::RED,
                            },
                        );
                    }
                    Entity::Wall => todo!(),
                }
            }
        }
    }
}
