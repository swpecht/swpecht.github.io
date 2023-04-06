use crate::{
    cfragent::CFRNode,
    database::{file_backend::FileBackend, NodeStore},
    game::{GameState, Player},
};

pub struct VanillaCFR {
    nodes_touched: usize,
}

impl VanillaCFR {
    pub fn run<T: GameState>(
        &mut self,
        ns: &mut NodeStore<FileBackend>,
        gs: &T,
        update_player: Player,
    ) {
        self.vcfr(ns, gs, update_player, 0, 1.0, 1.0, 1.0);
    }

    // // This is Vanilla CFR. See Marc L's thesis, Algorithm 1 (Section 2.2.2)
    fn vcfr<T: GameState>(
        &mut self,
        ns: &mut NodeStore<FileBackend>,
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

#[cfg(test)]
mod tests {
    use crate::{
        cfragent::cfr::VanillaCFR,
        database::{file_backend::FileBackend, NodeStore, Storage},
        game::{Action, GameState},
        istate::IStateKey,
        kuhn_poker::{KPAction, KuhnPoker},
    };

    #[test]
    fn vcfr_nash_test() {
        let game = KuhnPoker::game();
        // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
        let mut ns = NodeStore::new(FileBackend::new(Storage::Temp));
        let mut cfr = VanillaCFR::new();
        let gs = (game.new)();

        for _ in 0..10000 {
            cfr.vcfr(&mut ns, &gs, 0, 0, 1.0, 1.0, 1.0);
            cfr.vcfr(&mut ns, &gs, 1, 0, 1.0, 1.0, 1.0);
        }

        // The second player has a single equilibrium strategy:
        // Always betting or calling when having a King
        // 2b
        let k = get_key(&[2, 0, KPAction::Bet as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        // 2p
        let k = get_key(&[2, 0, KPAction::Pass as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Bet as usize], 1.0, 2);

        // when having a Queen, checking if possible, otherwise calling with the probability of 1/3
        // 1p
        let k = get_key(&[1, 0, KPAction::Pass as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        // 1b
        let k = get_key(&[1, 0, KPAction::Bet as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Bet as usize], 0.3333, 2);

        // when having a Jack, never calling and betting with the probability of 1/3.
        // 0b
        let k = get_key(&[0, 1, KPAction::Bet as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);
        // 0p
        let k = get_key(&[0, 1, KPAction::Pass as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Bet as usize], 0.3333, 2);

        // First player equilibrium
        // In one possible formulation, player one freely chooses the probability
        // {\displaystyle \alpha \in [0,1/3]}{\displaystyle \alpha \in [0,1/3]}
        // with which he will bet when having a Jack (otherwise he checks; if the
        //other player bets, he should always fold).
        // 0
        let k = get_key(&[0]);
        let alpha = get_policy(&mut ns, &k)[KPAction::Bet as usize];
        assert!(alpha < 0.4);

        // 0pb
        let k = get_key(&[0, 1, KPAction::Pass as usize, KPAction::Bet as usize]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        // When having a King, he should bet with the probability of {\displaystyle 3\alpha }3\alpha
        // (otherwise he checks; if the other player bets, he should always call)
        // 2
        let k = get_key(&[2]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Bet as usize], 3.0 * alpha, 2);

        // He should always check when having a Queen, and if the other player bets after this check,
        // he should call with the probability of {\displaystyle \alpha +1/3}{\displaystyle \alpha +1/3}.
        // 1
        let k = get_key(&[1]);
        let w = get_policy(&mut ns, &k);
        check_floats(w[KPAction::Pass as usize], 1.0, 2);

        // 1pb
        let k = get_key(&[1, 0, KPAction::Pass as usize, KPAction::Bet as usize]);
        let w = get_policy(&mut ns, &k);
        // We nudge the optimal weight here to save on iterations for convergence
        check_floats(w[KPAction::Bet as usize], alpha + 0.35, 2);
    }

    fn check_floats(x: f32, y: f32, i: i32) {
        let diff = (x * (10.0f32).powi(i)) - (y * (10.0f32).powi(i));

        if diff > 2.0 {
            panic!("got: {} expected: {}", x, y);
        }
    }

    /// Gets a key for player 0 of a new gamestate after applying the passed actions
    fn get_key(actions: &[Action]) -> IStateKey {
        let mut g = (KuhnPoker::game().new)();
        for &a in actions {
            g.apply_action(a);
        }

        return g.istate_key(0);
    }

    fn get_policy(ns: &mut NodeStore<FileBackend>, istate: &IStateKey) -> Vec<f32> {
        let n = ns.get_node_mut(istate).unwrap();
        let p = n.get_average_strategy();
        return p;
    }
}
