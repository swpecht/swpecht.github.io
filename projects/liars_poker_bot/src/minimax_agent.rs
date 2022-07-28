use log::debug;
use rand::{prelude::SliceRandom, thread_rng};

use crate::{agents::Agent, game::GameState, game_tree::GameTree, liars_poker::Player};

pub struct MinimaxAgent {}

impl<G: Clone> Agent<G> for MinimaxAgent
where
    G: GameState + std::fmt::Debug,
    <G as GameState>::Action: std::fmt::Debug,
{
    fn name(&self) -> &str {
        return "MinimaxAgent";
    }

    fn play(&self, g: &G, possible_moves: &Vec<G::Action>) -> G::Action {
        let acting_player = g.get_acting_player();

        let mut cur_max = match acting_player {
            Player::P1 => f32::MIN,
            Player::P2 => f32::MAX,
        };
        let mut cur_move = None;
        let mut rng = thread_rng();
        Finish shuffling moves so get a random best move
        for a in possible_moves.shuffle(&mut rng).iter() {
            let mut f = g.clone();
            f.apply(&a);
            debug!("Evaluating: {:?}", f);
            let t = GameTree::new(&f);
            print!("{:?}", t);

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

        return cur_move.unwrap().clone();
    }
}
