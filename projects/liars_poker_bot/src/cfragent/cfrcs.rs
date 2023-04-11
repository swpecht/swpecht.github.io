use log::{debug, trace};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{
    database::{file_backend::FileBackend, FileNodeStore, NodeStore},
    game::{GameState, Player},
};

use super::{cfr::Algorithm, CFRNode};

/// Implementation of chance sampled CFR
///
/// Based on implementation from: http://mlanctot.info/
/// cfrcs.cpp
pub struct CFRCS {
    nodes_touched: usize,
    rng: StdRng,
}

impl Algorithm for CFRCS {
    fn run<T: GameState, N: NodeStore>(&mut self, ns: &mut N, gs: &T, update_player: Player) {
        self.cfrcs(ns, gs, update_player, 0, 1.0, 1.0);
    }
}

impl CFRCS {
    pub fn new(seed: u64) -> Self {
        Self {
            nodes_touched: 0,
            rng: SeedableRng::seed_from_u64(seed),
        }
    }

    fn cfrcs<T: GameState, N: NodeStore>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
        depth: usize,
        reach0: f32,
        reach1: f32,
    ) -> f32 {
        if self.nodes_touched % 1000000 == 0 {
            debug!("cfr touched {} nodes", self.nodes_touched);
        }
        self.nodes_touched += 1;

        if gs.is_terminal() {
            return gs.evaluate()[update_player].into();
        }

        let cur_player = gs.cur_player();
        let actions = gs.legal_actions();
        if actions.len() == 1 {
            // avoid processing nodes with no choices
            let mut ngs = gs.clone();
            ngs.apply_action(actions[0]);
            return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1);
        }

        if gs.is_chance_node() {
            let a = *actions.choose(&mut self.rng).unwrap();
            let mut ngs = gs.clone();
            ngs.apply_action(a);
            return self.cfrcs(ns, &ngs, update_player, depth + 1, reach0, reach1);
        }

        let is = gs.istate_key(gs.cur_player());

        // log the call
        trace!("cfr processing:\t{}", is.to_string());
        trace!("node:\t{}", gs);
        let mut strat_ev = 0.0;

        let mut move_evs = Vec::new();
        for _ in 0..actions.len() {
            move_evs.push(0.0);
        }

        let mut node = ns.get_node_mut(&is).unwrap_or(CFRNode::new(is, &actions));
        let param = match cur_player {
            0 | 2 => reach0,
            1 | 3 => reach1,
            _ => panic!("invalid player"),
        };
        let move_prob = node.get_move_prob(param);

        // // iterate over the actions
        for &a in &actions {
            let idx = node.get_index(a);

            let newreach0 = match gs.cur_player() {
                0 | 2 => reach0 * move_prob[idx],
                1 | 3 => reach0,
                _ => panic!("invalid player"),
            };

            let newreach1 = match gs.cur_player() {
                0 | 2 => reach1,
                1 | 3 => reach1 * move_prob[idx],
                _ => panic!("invalid player"),
            };

            let mut ngs = gs.clone();
            ngs.apply_action(a);
            let payoff = self.cfrcs(ns, &ngs, update_player, depth + 1, newreach0, newreach1);
            move_evs[idx] = payoff;
            strat_ev += move_prob[idx] * payoff;
        }

        // // post-traversals: update the infoset
        if cur_player == update_player {
            let (my_reach, opp_reach) = match gs.cur_player() {
                0 | 2 => (reach0, reach1),
                1 | 3 => (reach1, reach0),
                _ => panic!("invalid player"),
            };

            for a in actions {
                let idx = node.get_index(a);
                node.regret_sum[idx] += opp_reach * (move_evs[idx] - strat_ev);
                node.total_move_prob[idx] += my_reach * node.move_prob[idx]
            }

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
        _test_kp_nash(CFRCS::new(42), 50000)
    }
}
