use card_platypus::algorithms::{
    ismcts::Evaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot,
};
use games::{
    actions,
    gamestates::euchre::{actions::EAction, Euchre, EuchreGameState},
    GameState,
};
use itertools::Itertools;
use log::info;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::Args;

pub fn open_hand_score_pass_on_bower(_args: Args) {
    let rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = PIMCTSBot::new(100, OpenHandSolver::new_euchre(), rng);

    info!("iterating through pass on the bower nodes");
    for mut gs in PassOnBowerIterator::new() {
        gs.apply_action(EAction::Pass.into());
        let pass_value = evaluator.evaluate_player(&gs, 3);
        gs.undo();
        gs.apply_action(EAction::Pickup.into());
        let pickup_value = evaluator.evaluate_player(&gs, 3);
        gs.undo();
        info!(
            "policy evaluation\t{}\t{}\t{}",
            gs, pass_value, pickup_value
        )
    }
}

pub fn spot_check_pass_on_bower(_args: Args) {
    let rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = PIMCTSBot::new(10000, OpenHandSolver::new_euchre(), rng);

    let games = vec![
        "JCAC9STSQS|KSASTHJHQH|KHAH9DTDJD|9CTCQCKC9H|JS|PPP",
        "JC9STSQSKS|AS9HTHJHQH|KHAH9DTDJD|9CTCQCKCAC|JS|PPP",
        "JCKC9STSQS|KSAS9HTHJH|KHAH9DTDJD|9CTCQCACQH|JS|PPP",
        "JCAC9STSQS|KSAS9HTHJH|QHKHAH9DTD|9CTCQCKCQD|JS|PPP",
        "TCJC9STSQS|KSAS9HTHJH|QHKHAH9DTD|9CQCKCACQD|JS|PPP",
        "9CJC9STSQS|KSAS9HTHJH|QHKHAH9DTD|TCQCKCACKD|JS|PPP",
        "TCJC9STSQS|KSAS9HJHQH|KHAH9DTDJD|9CQCKCACTH|JS|PPP",
        "9CJC9STSQS|KSAS9HJHQH|KHAH9DTDJD|TCQCKCACTH|JS|PPP",
        "JCAC9STSQS|KSAS9HTHQH|KHAH9DTDJD|9CTCQCKCJH|JS|PPP",
        "9CJC9STSQS|KSAS9HTHJH|QHKHAH9DTD|TCQCKCACQD|JS|PPP",
        "TCJC9STSQS|KSASTHJHQH|KHAH9DTDJD|9CQCKCAC9H|JS|PPP",
        "TCJC9STSQS|KSAS9HTHJH|QHKHAH9DJD|9CQCKCACTD|JS|PPP",
        "9CJC9STSQS|KSASTHJHQH|KHAH9DTDJD|TCQCKCAC9H|JS|PPP",
        "9CJC9STSQS|KSAS9HTHJH|QHAH9DTDJD|TCQCKCACKH|JS|PPP",
        "9CJCAC9STS|QSKSASTHQH|KHAH9DTDJD|TCQCKC9HJH|JS|PPP",
        "9CJCAC9STS|QSKSAS9HTH|JHQHKHAH9D|TCQCKCJDQD|JS|PPP",
        "JCKCAC9STS|QSKSASTHJH|QHKHAH9DTD|9CTCQC9HQD|JS|PPP",
        "JCQC9STSQS|KSAS9HTHJH|QHKHAH9DTD|9CTCKCACQD|JS|PPP",
        "TCJC9STSQS|KSAS9HTHQH|KHAH9DTDJD|9CQCKCACJH|JS|PPP",
        "JCKCAC9STS|QSKSAS9HTH|JHKHAH9DJD|9CTCQCQHTD|JS|PPP",
        "JCKC9STSQS|KSASTHJHQH|KHAH9DTDJD|9CTCQCAC9H|JS|PPP",
        "JCAC9STSQS|KSAS9HTHJH|QHKHAH9DJD|9CTCQCKCTD|JS|PPP",
        "JCQC9STSQS|KSAS9HTHJH|QHAH9DTDJD|9CTCKCACKH|JS|PPP",
        "TCJCAC9STS|QSKSAS9HJH|QHKHAH9DTD|9CQCKCTHJD|JS|PPP",
        "JCAC9STSQS|KSAS9HJHQH|KHAH9DTDJD|9CTCQCKCTH|JS|PPP",
        "9CJC9STSQS|KSAS9HTHQH|KHAH9DTDJD|TCQCKCACJH|JS|PPP",
        "JCKC9STSQS|KSAS9HTHJH|QHKHAH9DTD|9CTCQCACQD|JS|PPP",
        "JCQC9STSQS|KSAS9HTHJH|QHKHAH9DTD|9CTCKCACJD|JS|PPP",
        "JCAC9STSQS|KSAS9HTHJH|QHAH9DTDJD|9CTCQCKCKH|JS|PPP",
        "9CJC9STSQS|KSAS9HTHJH|QHKHAH9DTD|TCQCKCACJD|JS|PPP",
    ];
    for s in games {
        let mut gs = EuchreGameState::from(s);
        gs.apply_action(EAction::Pass.into());
        let pass_value = evaluator.evaluate_player(&gs, 3);
        gs.undo();
        gs.apply_action(EAction::Pickup.into());
        let pickup_value = evaluator.evaluate_player(&gs, 3);
        gs.undo();
        info!(
            "policy evaluation\t{}\t{}\t{}",
            gs, pass_value, pickup_value
        )
    }
}

pub fn calculate_open_hand_solver_convergence(_args: Args) {
    info!("calculating evaluator converge");

    let mut rng: StdRng = SeedableRng::seed_from_u64(42);
    let rollouts: Vec<usize> = vec![1, 10, 100, 1000, 10000];
    let mut evaluators = rollouts
        .iter()
        .map(|x| PIMCTSBot::new(*x, OpenHandSolver::new_euchre(), rng.clone()))
        .collect_vec();
    info!("rollouts: {:?}", rollouts);

    let generator = PassOnBowerIterator::new();
    let mut worlds = generator.collect_vec();
    worlds.shuffle(&mut rng);

    for gs in worlds.iter().take(1000) {
        let mut gs = gs.clone();
        gs.apply_action(EAction::Pickup.into());
        let mut results = Vec::new();
        for (e, _r) in evaluators.iter_mut().zip(rollouts.iter()) {
            let v = e.evaluate_player(&gs, 3);
            results.push(v);
        }

        info!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            gs, results[0], results[1], results[2], results[3], results[4]
        );
    }
}

pub struct PassOnBowerIterator {
    hands: Vec<[EAction; 5]>,
}

impl PassOnBowerIterator {
    pub fn new() -> Self {
        todo!("re-implement with the card representation")
        // let mut hands = Vec::new();
        // // todo: rewrite with combination function?
        // for a in 0..20 {
        //     for b in a + 1..21 {
        //         for c in b + 1..22 {
        //             for d in c + 1..23 {
        //                 for e in d + 1..24 {
        //                     if a == u8::from(Card::JS)
        //                         || b == u8::from(Card::JS)
        //                         || c == u8::from(Card::JS)
        //                         || d == u8::from(Card::JS)
        //                         || e == u8::from(Card::JS)
        //                     {
        //                         continue;
        //                     }
        //                     hands.push([
        //                         EAction::private_action(a.into()),
        //                         EAction::private_action(b.into()),
        //                         EAction::private_action(c.into()),
        //                         EAction::private_action(d.into()),
        //                         EAction::private_action(e.into()),
        //                     ])
        //                 }
        //             }
        //         }
        //     }
        // }
        // hands.reverse();
        // Self { hands }
    }
}

impl Default for PassOnBowerIterator {
    fn default() -> Self {
        Self::new()
    }
}

impl Iterator for PassOnBowerIterator {
    type Item = EuchreGameState;

    fn next(&mut self) -> Option<Self::Item> {
        let jack = EAction::JS;
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
            gs.apply_action(EAction::JS.into());
            gs.apply_action(EAction::Pass.into());
            gs.apply_action(EAction::Pass.into());
            gs.apply_action(EAction::Pass.into());

            return Some(gs);
        }

        None
    }
}
