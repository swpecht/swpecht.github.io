use log::debug;

use crate::{
    agents::Agent,
    game_tree::GameTree,
    liars_poker::{apply_action, get_winner, DiceState, GameState, Player, DICE_SIDES, NUM_DICE},
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
        let mut cur_max = f32::MIN;
        let mut cur_move = None;
        for a in possible_moves {
            let f = apply_action(g, a);
            debug!("Evaluating: {:?}", f);
            // let value = minimax(g, &mut f32::MIN, &mut f32::MAX, true);
            let t = GameTree::new(&f);
            // print!("{:?}", t);

            let value = t.get(0).score.unwrap();

            debug!("value: {:?}", value);
            if value > cur_max {
                cur_max = value;
                cur_move = Some(a)
            }
        }

        return *cur_move.unwrap();
    }
}
