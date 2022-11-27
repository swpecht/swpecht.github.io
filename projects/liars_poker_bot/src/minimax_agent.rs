use log::debug;
use rand::{prelude::SliceRandom, thread_rng};

use crate::{agents::Agent, game::GameState, game_tree::GameTree, liars_poker::Player};

pub struct MinimaxAgent {}

impl<G: Clone> Agent<G> for MinimaxAgent
where
    G: GameState + std::fmt::Debug,
{
    fn name(&self) -> &str {
        return "MinimaxAgent";
    }

    fn play(&self, g: &G, possible_moves: &Vec<G>) -> G {
        let p = g.get_acting_player();

        let mut cur_max = match p {
            Player::P1 => f32::MIN,
            Player::P2 => f32::MAX,
        };
        let mut cur_move = None;

        // Shuffle the order of evaluating moves to choose a random one if multiple have
        // the same utility
        let mut rng = thread_rng();
        let mut shuffled_moves = possible_moves.clone();
        shuffled_moves.shuffle(&mut rng);
        for g_next in shuffled_moves {
            debug!("Evaluating: {:?}", g_next);
            let t = GameTree::new(&g_next);
            // print!("{:?}", t);

            let value = t.get(0).score.unwrap();

            debug!("value: {:?}", value);

            let is_better = match p {
                Player::P1 => value > cur_max,
                Player::P2 => value < cur_max,
            };

            if is_better {
                cur_max = value;
                cur_move = Some(g_next)
            }
        }

        return cur_move.unwrap().clone();
    }
}
