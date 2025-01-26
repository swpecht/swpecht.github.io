use super::{SimState, Team};

impl std::hash::Hash for SimState {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.next_model_id.hash(state);
        self.initiative.hash(state);
        self.locations.hash(state);
        // self.models.hash(state);
        self.is_start_of_turn.hash(state);
        todo!()
    }
}

impl SimState {
    pub fn evaluate(&self, team: Team) -> i32 {
        const WIN_VALUE: i32 = 0; //  1000.0;
                                  // todo: add score component for entity count

        // TODO: include wounds in this? Easier to differentiate
        let mut player_models = 0;
        let mut npc_models = 0;
        for entity in self.models.iter().filter(|e| !e.is_destroyed) {
            match entity.team {
                Team::Players => player_models += 1,
                Team::NPCs => npc_models += 1,
            }
        }

        let model_score = match team {
            Team::Players => player_models - npc_models,
            Team::NPCs => npc_models - player_models,
        };

        let win_score = match (team, player_models, npc_models) {
            (Team::Players, 0, _) => -WIN_VALUE,
            (Team::Players, _, 0) => WIN_VALUE,
            (Team::NPCs, 0, _) => WIN_VALUE,
            (Team::NPCs, _, 0) => -WIN_VALUE,
            (_, _, _) => 0,
        };

        model_score + win_score
    }

    pub fn is_start_of_turn(&self) -> bool {
        self.is_start_of_turn
    }
}
