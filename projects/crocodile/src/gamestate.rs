pub struct SimState {
    grid: Vec<Vec<Option<Entity>>>,
}

#[derive(Clone, Copy)]
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
struct Entity {
    id: usize,
    actions: Vec<Action>,
}

impl Default for SimState {
    fn default() -> Self {
        let player = Entity {
            id: 0,
            actions: vec![Action::MoveUp, Action::EndTurn],
        };
        let mut grid = vec![vec![None; 100]; 100];
        grid[50][50] = Some(player);

        Self { grid }
    }
}

impl SimState {
    pub fn apply(&mut self, action: &Action) {
        todo!()
    }
}
