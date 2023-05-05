use std::{cell::RefCell, rc::Rc};

use log::{debug, trace};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    database::NodeStore,
    game::{Action, GameState, Player},
};

use super::{
    cfr::{Algorithm, CFRPhase},
    CFRNode,
};

/// Implementation of chance sampled CFR
///
/// Based on implementation from: http://mlanctot.info/
/// cfrcs.cpp
pub struct CFRCS {
    nodes_touched: usize,
    rng: StdRng,
    vector_pool: Pool<Vec<Action>>,
}

impl Algorithm for CFRCS {
    fn run<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
    ) {
        self.cfrcs(ns, gs, update_player, 0, 1.0, 1.0, CFRPhase::Phase1);
    }

    fn nodes_touched(&self) -> usize {
        return self.nodes_touched;
    }
}

impl CFRCS {
    pub fn new(seed: u64) -> Self {
        Self {
            nodes_touched: 0,
            rng: SeedableRng::seed_from_u64(seed),
            vector_pool: Pool::new(|| Vec::new()),
        }
    }

    fn cfrcs<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
        depth: usize,
        reach0: f64,
        reach1: f64,
        mut phase: CFRPhase,
    ) -> f64 {
        if self.nodes_touched % 1000000 == 0 {
            debug!("cfr touched {} nodes", self.nodes_touched);
        }
        self.nodes_touched += 1;

        if gs.is_terminal() {
            return gs.evaluate(update_player);
        }

        let cur_player = gs.cur_player();
        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);
        if actions.len() == 1 {
            // avoid processing nodes with no choices
            let mut ngs = gs.clone();
            ngs.apply_action(actions[0]);
            return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1, phase);
        }

        if gs.is_chance_node() {
            let a = *actions.choose(&mut self.rng).unwrap();
            let mut ngs = gs.clone();
            ngs.apply_action(a);
            return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1, phase);
        }

        // check for cuts  (pruning optimization from Section 2.2.2) of Marc's thesis
        let team = match cur_player {
            0 | 2 => 0,
            1 | 3 => 1,
            _ => panic!("invald player"),
        };
        let update_team = match update_player {
            0 | 2 => 0,
            1 | 3 => 1,
            _ => panic!("invald player"),
        };

        if phase == CFRPhase::Phase1
            && ((team == 0 && update_team == 0 && reach1 <= 0.0)
                || (team == 1 && update_team == 1 && reach0 <= 0.0))
        {
            phase = CFRPhase::Phase2;
        }

        if phase == CFRPhase::Phase2
            && ((team == 0 && update_team == 0 && reach0 <= 0.0)
                || (team == 1 && update_team == 1 && reach1 <= 0.0))
        {
            trace!("pruning cfr tree");
            return 0.0;
        }

        let is = gs.istate_key(gs.cur_player());

        // log the call
        trace!("cfr processing:\t{}", is.to_string());
        trace!("node:\t{}", gs);
        let mut strat_ev = 0.0;

        let mut move_evs = ActionVec::new(&actions);

        let node = ns
            .get(&is)
            .unwrap_or(Rc::new(RefCell::new(CFRNode::new(actions.clone()))));
        let param = match cur_player {
            0 | 2 => reach0,
            1 | 3 => reach1,
            _ => panic!("invalid player"),
        };

        // // iterate over the actions
        let move_probs = node.borrow_mut().get_move_prob(param);
        for &a in &actions {
            let newreach0 = match gs.cur_player() {
                0 | 2 => reach0 * move_probs[a],
                1 | 3 => reach0,
                _ => panic!("invalid player"),
            };

            let newreach1 = match gs.cur_player() {
                0 | 2 => reach1,
                1 | 3 => reach1 * move_probs[a],
                _ => panic!("invalid player"),
            };

            let mut ngs = gs.clone();
            ngs.apply_action(a);
            let payoff = self.cfrcs(
                ns,
                &ngs,
                update_player,
                depth + 1,
                newreach0,
                newreach1,
                phase,
            );
            move_evs[a] = payoff;
            strat_ev += move_probs[a] * payoff;
        }

        let (my_reach, opp_reach) = match gs.cur_player() {
            0 | 2 => (reach0, reach1),
            1 | 3 => (reach1, reach0),
            _ => panic!("invalid player"),
        };

        // // post-traversals: update the infoset
        if phase == CFRPhase::Phase1 && cur_player == update_player {
            for &a in &actions {
                node.borrow_mut().regret_sum[a] += opp_reach * (move_evs[a] - strat_ev);
            }
        }

        if phase == CFRPhase::Phase2 && cur_player == update_player {
            for a in actions {
                let mut n = node.borrow_mut();
                n.total_move_prob[a] += my_reach * n.move_prob[a];
            }
        }

        // todo: figure out if need the explicit updates
        if cur_player == update_player {
            ns.insert_node(is, node);
        }

        return strat_ev;
    }
}

#[cfg(test)]
mod tests {
    use crate::cfragent::cfr::_test_kp_nash;

    use super::CFRCS;

    #[test]
    fn cfrcs_nash_test() {
        _test_kp_nash(CFRCS::new(5), 50000)
    }
}
