use log::debug;

use crate::{
    agents::Agent,
    game_tree::GameTree,
    liars_poker::{apply_action, get_acting_player, Player},
};

pub struct MinimaxAgent {
    pub name: String,
}

impl Agent for MinimaxAgent {
    fn name(&self) -> &str {
        return &self.name;
    }

    fn play(
        &self,
        g: &crate::liars_poker::GameState,
        possible_moves: &Vec<crate::liars_poker::Action>,
    ) -> crate::liars_poker::Action {
        let acting_player = get_acting_player(g);

        let mut cur_max = match acting_player {
            Player::P1 => f32::MIN,
            Player::P2 => f32::MAX,
        };
        let mut cur_move = None;
        for a in possible_moves {
            let f = apply_action(g, a);
            debug!("Evaluating: {:?}", f);
            let t = GameTree::new(&f);
            // print!("{:?}", t);

            let value = t.get(0).score.unwrap();

            debug!("value: {:?}", value);

            let is_better = match acting_player {
                Player::P1 => value > cur_max,
                Player::P2 => value < cur_max,
            };

            if is_better {
                cur_max = value;
                cur_move = Some(a)
            }
        }

        return *cur_move.unwrap();
    }
}
