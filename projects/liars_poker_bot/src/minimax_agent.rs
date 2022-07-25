use itertools::Itertools;
use log::debug;

use crate::{
    agents::Agent,
    game_tree::GameTree,
    liars_poker::{
        apply_action, get_acting_player, get_last_bet, get_possible_actions, DiceState, LPAction,
        LPGameState, Player, DICE_SIDES, NUM_DICE,
    },
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

/// Runs minimax based on other players moves to guess their
/// dice.
pub struct MetaMinimaxAgent {}

impl Agent<LPGameState> for MetaMinimaxAgent {
    fn name(&self) -> &str {
        return "MetaMiniMaxAgent";
    }

    fn play(
        &self,
        g: &LPGameState,
        possible_moves: &Vec<crate::liars_poker::LPAction>,
    ) -> LPAction {
        let mma = MinimaxAgent {};

        // Get previous state
        let lb_index = get_last_bet(g);
        if lb_index == None {
            // If no previous bet, just play Minimax
            return mma.play(g, possible_moves);
        }
        let lb_index = lb_index.unwrap();
        let last_bet = LPAction::Bet(lb_index);
        let mut pg = g.clone();
        pg.bet_state[lb_index] = None; // Remove last bet
        pg.dice_state = [DiceState::U; NUM_DICE]; // remove dice

        // Test possible dice and see which state gives the same minimax action
        let unknown_dice = (0..NUM_DICE / 2)
            .map(|_| 0..DICE_SIDES)
            .multi_cartesian_product();

        let mut found_match = false;
        for dice_guess in unknown_dice {
            // Set the dice state
            for (i, &v) in dice_guess.iter().enumerate() {
                pg.dice_state[i] = DiceState::K(v);
            }

            debug!("Meta agent testing dicestate {:?}", pg.dice_state);
            let a = mma.play(&pg, &get_possible_actions(&pg));
            debug!("Meta agent suggests action {:?}", a);
            if a == last_bet {
                found_match = true;
                break;
            }
        }

        let mut eg = g.clone();
        if found_match {
            let known_dice = g
                .dice_state
                .iter()
                .filter(|&x| matches!(x, DiceState::K(_)))
                .collect_vec();
            for (i, &d) in known_dice.iter().enumerate() {
                eg.dice_state[i + NUM_DICE / 2] = *d;
            }
            debug!(
                "Meta agent found a match, estimated dice state: {:?}",
                eg.dice_state
            );
        }
        return mma.play(&eg, possible_moves);
    }
}
