use crate::{
    cfragent::cfrnode::CFRNode,
    database::{memory_node_store::MemoryNodeStore, NodeStore},
    game::kuhn_poker::{KPAction, KuhnPoker},
    game::{Action, GameState, Player},
    istate::IStateKey,
};

pub trait Algorithm {
    fn run<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
    );
    fn nodes_touched(&self) -> usize;
}

pub struct VanillaCFR {
    nodes_touched: usize,
}

impl Algorithm for VanillaCFR {
    fn run<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
    ) {
        self.vcfr(ns, gs, update_player, 0, 1.0, 1.0, 1.0);
    }

    fn nodes_touched(&self) -> usize {
        return self.nodes_touched;
    }
}

impl VanillaCFR {
    fn vcfr<T: GameState, N: NodeStore<CFRNode>>(
        &mut self,
        ns: &mut N,
        gs: &T,
        update_player: Player,
        depth: usize,
        reach0: f32,
        reach1: f32,
        chance_reach: f32,
    ) -> f32 {
        let cur_player = gs.cur_player();
        if gs.is_terminal() {
            return gs.evaluate()[update_player].into();
        }
        self.nodes_touched += 1;

        if gs.is_chance_node() {
            let mut ev = 0.0;

            let actions = &gs.legal_actions();
            for &a in actions {
                let mut ngs = gs.clone();
                ngs.apply_action(a);

                let chance_prob = 1.0 / actions.len() as f32;
                let new_chance_reach = chance_prob * chance_reach;
                ev += chance_prob
                    * self.vcfr(
                        ns,
                        &ngs,
                        update_player,
                        depth + 1,
                        reach0,
                        reach1,
                        new_chance_reach,
                    );
            }
            return ev;
        }

        let is = gs.istate_key(gs.cur_player());
        let mut strat_ev = 0.0;

        let actions = gs.legal_actions();

        let mut move_evs = Vec::new();
        for _ in 0..actions.len() {
            move_evs.push(0.0);
        }

        let mut node = ns.get_node_mut(&is).unwrap_or(CFRNode::new(&actions));
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
            let payoff = self.vcfr(
                ns,
                &ngs,
                update_player,
                depth + 1,
                newreach0,
                newreach1,
                chance_reach,
            );
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
                node.regret_sum[idx] += (chance_reach * opp_reach) * (move_evs[idx] - strat_ev);
                node.total_move_prob[idx] += my_reach * node.move_prob[idx]
            }

            ns.insert_node(is, node);
        }
        return strat_ev;
    }

    pub fn new() -> Self {
        Self { nodes_touched: 0 }
    }
}

/// Returns the policy of a given istate
fn _get_policy<T: NodeStore<CFRNode>>(ns: &mut T, istate: &IStateKey) -> Vec<f32> {
    let n = ns.get_node_mut(istate).unwrap();
    let p = n.get_average_strategy();
    return p;
}

fn _check_floats(x: f32, y: f32, i: i32) {
    let diff = (x * (10.0f32).powi(i)) - (y * (10.0f32).powi(i));

    if diff > 2.0 {
        panic!("got: {} expected: {}", x, y);
    }
}

/// Gets a key for player 0 of a new gamestate after applying the passed actions
fn _get_key(actions: &[Action]) -> IStateKey {
    let g = KuhnPoker::from_actions(actions);

    return g.istate_key(0);
}

pub(super) fn _test_kp_nash<T: Algorithm>(mut alg: T, iterations: usize) {
    let game = KuhnPoker::game();
    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    let mut ns = MemoryNodeStore::new();
    let gs = (game.new)();

    for _ in 0..iterations {
        alg.run(&mut ns, &gs, 0);
        alg.run(&mut ns, &gs, 1);
    }

    // The second player has a single equilibrium strategy:
    // Always betting or calling when having a King
    // 2b
    let k = _get_key(&[2, 0, KPAction::Bet as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Bet as usize], 1.0, 2);

    // 2p
    let k = _get_key(&[2, 0, KPAction::Pass as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Bet as usize], 1.0, 2);

    // when having a Queen, checking if possible, otherwise calling with the probability of 1/3
    // 1p
    let k = _get_key(&[1, 0, KPAction::Pass as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Pass as usize], 1.0, 2);
    // 1b
    let k = _get_key(&[1, 0, KPAction::Bet as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Bet as usize], 0.3333, 2);

    // when having a Jack, never calling and betting with the probability of 1/3.
    // 0b
    let k = _get_key(&[0, 1, KPAction::Bet as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Pass as usize], 1.0, 2);
    // 0p
    let k = _get_key(&[0, 1, KPAction::Pass as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Bet as usize], 0.3333, 2);

    // First player equilibrium
    // In one possible formulation, player one freely chooses the probability
    // {\displaystyle \alpha \in [0,1/3]}{\displaystyle \alpha \in [0,1/3]}
    // with which he will bet when having a Jack (otherwise he checks; if the
    //other player bets, he should always fold).
    // 0
    let k = _get_key(&[0]);
    let alpha = _get_policy(&mut ns, &k)[KPAction::Bet as usize];
    assert!(alpha < 0.4);

    // 0pb
    let k = _get_key(&[0, 1, KPAction::Pass as usize, KPAction::Bet as usize]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Pass as usize], 1.0, 2);

    // When having a King, he should bet with the probability of {\displaystyle 3\alpha }3\alpha
    // (otherwise he checks; if the other player bets, he should always call)
    // 2
    let k = _get_key(&[2]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Bet as usize], 3.0 * alpha, 2);

    // He should always check when having a Queen, and if the other player bets after this check,
    // he should call with the probability of {\displaystyle \alpha +1/3}{\displaystyle \alpha +1/3}.
    // 1
    let k = _get_key(&[1]);
    let w = _get_policy(&mut ns, &k);
    _check_floats(w[KPAction::Pass as usize], 1.0, 2);

    // 1pb
    let k = _get_key(&[1, 0, KPAction::Pass as usize, KPAction::Bet as usize]);
    let w = _get_policy(&mut ns, &k);
    // We nudge the optimal weight here to save on iterations for convergence
    _check_floats(w[KPAction::Bet as usize], alpha + 0.35, 2);
}

#[cfg(test)]
mod tests {
    use crate::cfragent::cfr::VanillaCFR;

    use super::_test_kp_nash;

    #[test]
    fn vcfr_nash_test() {
        _test_kp_nash(VanillaCFR::new(), 10000)
    }
}
