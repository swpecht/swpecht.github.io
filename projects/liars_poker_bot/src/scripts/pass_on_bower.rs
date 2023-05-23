use liars_poker_bot::{
    actions,
    algorithms::open_hand_solver::OpenHandSolver,
    game::{
        euchre::{
            actions::{Card, EAction},
            Euchre, EuchreGameState,
        },
        GameState,
    },
    policy::Policy,
};
use log::info;
use rand::{rngs::StdRng, SeedableRng};

use crate::Args;

pub fn open_hand_score_pass_on_bower(_args: Args) {
    let rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = OpenHandSolver::new(100, rng);

    info!("iterating through pass on the bower nodes");
    for gs in PassOnBowerIterator::new() {
        let policy = evaluator.action_probabilities(&gs);
        info!(
            "policy evaluation\t{}\t{}\t{}",
            gs,
            policy[EAction::Pass.into()],
            policy[EAction::Pickup.into()]
        )
    }
}

pub struct PassOnBowerIterator {
    hands: Vec<[EAction; 5]>,
}

impl PassOnBowerIterator {
    fn new() -> Self {
        let mut hands = Vec::new();
        // todo: rewrite with combination function?
        for a in 0..20 {
            for b in a + 1..21 {
                for c in b + 1..22 {
                    for d in c + 1..23 {
                        for e in d + 1..24 {
                            if a == Card::JS.into()
                                || b == Card::JS.into()
                                || c == Card::JS.into()
                                || d == Card::JS.into()
                                || e == Card::JS.into()
                            {
                                continue;
                            }
                            hands.push([
                                EAction::DealPlayer { c: a.into() },
                                EAction::DealPlayer { c: b.into() },
                                EAction::DealPlayer { c: c.into() },
                                EAction::DealPlayer { c: d.into() },
                                EAction::DealPlayer { c: e.into() },
                            ])
                        }
                    }
                }
            }
        }
        hands.reverse();
        Self { hands }
    }
}

impl Iterator for PassOnBowerIterator {
    type Item = EuchreGameState;

    fn next(&mut self) -> Option<Self::Item> {
        let jack = EAction::DealPlayer { c: Card::JS };
        if let Some(hand) = self.hands.pop() {
            let mut gs = Euchre::new_state();
            while gs.cur_player() != 3 {
                let actions = actions!(gs);
                for a in actions {
                    if !hand.contains(&a.into()) && EAction::from(a) != jack {
                        gs.apply_action(a);
                        break;
                    }
                }
            }

            // deal the dealers hands
            for c in hand {
                gs.apply_action(c.into())
            }

            // deal the faceup card
            gs.apply_action(EAction::DealFaceUp { c: Card::JS }.into());
            gs.apply_action(EAction::Pass.into());
            gs.apply_action(EAction::Pass.into());
            gs.apply_action(EAction::Pass.into());

            return Some(gs);
        }

        None
    }
}
