use bevy::{log::tracing_subscriber::Layer, prelude::Resource};

#[derive(Resource)]
pub struct SimState {
    grid: Vec<Vec<Option<SimEntity>>>,
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    EndTurn,
    Attack {
        dmg: usize,
        range: usize,
        aoe: usize,
    },
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
}

#[derive(Clone)]
struct SimEntity {
    id: SimId,
    actions: Vec<Action>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SimId(usize);

#[derive(Debug, Clone, Copy)]
pub struct SimCoords {
    pub x: usize,
    pub y: usize,
}

impl Default for SimState {
    fn default() -> Self {
        let player = SimEntity {
            id: SimId(0),
            actions: vec![Action::MoveUp, Action::EndTurn],
        };
        let mut grid = vec![vec![None; 100]; 100];
        grid[0][0] = Some(player);

        Self { grid }
    }
}

impl SimState {
    pub fn apply(&mut self, action: &Action) {
        todo!()
    }

    pub fn get_entity(&self, coords: SimCoords) -> Option<SimId> {
        self.grid
            .get(coords.x)
            .and_then(|x| x.get(coords.y))
            .map(|x| x.as_ref().map(|x| x.id))?
    }
}
