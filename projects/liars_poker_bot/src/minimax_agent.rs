use log::debug;

use crate::{
    agents::Agent,
    game::GameState,
    game_tree::GameTree,
    liars_poker::{get_acting_player, LPAction, LPGameState, Player},
};

pub struct MinimaxAgent {}

impl Agent<LPGameState> for MinimaxAgent {
    fn name(&self) -> &str {
        return "MinimaxAgent";
    }

    fn play(&self, g: &LPGameState, possible_moves: &Vec<LPAction>) -> LPAction {
        let acting_player = get_acting_player(g);

        let mut cur_max = match acting_player {
            Player::P1 => f32::MIN,
            Player::P2 => f32::MAX,
        };
        let mut cur_move = None;
        for a in possible_moves {
            let mut f = g.clone();
            f.apply(&a);
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
