// https://aipokertutorial.com/agent-evaluation/

mod alwaysfirsttrainableagent;
mod normalizer;

use crate::{
    cfragent::bestresponse::normalizer::{NormalizerMap, NormalizerVector},
    database::NodeStore,
    game::{Action, GameState, Player},
    kuhn_poker::KPGameState,
};

pub struct BestResponse {
    /// Vector of possible private chance outcomes for a given game. For example
    /// in KuhnPoker, this would be the dealt cards [[0], [1], [2]]. In Euchre, this would be all
    /// possible hand states [[0, 1, 2, 3, 4], [0, 1, 2, 3, 5],  ...]
    ///
    /// For now we're ignoring how to handle the discard card in Euchre.
    opp_chance_outcomes: Vec<Action>,
}

impl BestResponse {
    pub fn new() -> Self {
        Self {
            opp_chance_outcomes: Vec::new(),
        }
    }

    /// Runs the best response algorithm
    pub fn compute_best_response<T: NodeStore>(
        &mut self,
        gs: KPGameState,
        fixed_player: Player,
        opp_reach: Vec<f64>,
        ns: &mut T,
    ) -> f64 {
        self.opp_chance_outcomes = gs.chance_outcomes(fixed_player);
        return self.expectimaxbr(gs, fixed_player, opp_reach, ns);
    }

    /// Implements the best response alogirhtm from Marc's thesis.
    ///
    /// Args:
    ///     gs: Gamestate
    ///     opp_reach: chance of reaching this istate given the corresponsding opp chance outcomes
    ///     fixed_player: Iterating player
    ///     ns: node store
    pub fn expectimaxbr<T: NodeStore>(
        &mut self,
        gs: KPGameState,
        fixed_player: Player,
        opp_reach: Vec<f64>,
        ns: &mut T,
    ) -> f64 {
        assert!(fixed_player == 0 || fixed_player == 1);

        let update_player = (fixed_player + 1) % gs.num_players();

        // opponent never plays here, should choose this
        if gs.cur_player() == update_player && opp_reach.iter().sum::<f64>() == 0.0 {
            return f64::NEG_INFINITY;
        }

        if gs.is_terminal() {
            if opp_reach.iter().sum::<f64>() == 0.0 {
                return f64::NEG_INFINITY;
            }

            let mut opp_dist = NormalizerVector::new();

            for i in 0..self.opp_chance_outcomes.len() {
                // TODO: this may need updated, unclear on what `getChanceProb()` is doing in the original version
                // oppDist.push_back(getChanceProb(fixed_player, oppChanceOutcomes[i])*oppReach[i]);
                opp_dist.push(1.0 / self.opp_chance_outcomes.len() as f64 * opp_reach[i]);
            }

            opp_dist.normalize();

            let mut exp_payoff = 0.0;

            for i in 0..self.opp_chance_outcomes.len() {
                let payoff = gs.get_payoff(fixed_player, self.opp_chance_outcomes[i]);

                // TODO: unclear what `CHKPROB` and `CHKDBL` are doing, may need other asserts
                // CHKPROB(oppDist[i]);
                // CHKDBL(payoff);
                exp_payoff += opp_dist[i] * payoff
            }

            return exp_payoff;
        }

        if gs.is_chance_node() {
            if gs.cur_player() == fixed_player {
                // filling with a dummy variable since this is never used
                let mut ngs = gs.clone();
                let a = gs.legal_actions()[0];
                ngs.apply_action(a);
                return self.expectimaxbr(ngs, fixed_player, opp_reach.clone(), ns);
            }

            let mut ev = 0.0;
            let cos = gs.chance_outcomes(fixed_player);
            let num_cos = cos.len();
            for oc in cos {
                let mut ngs = gs.clone();
                ngs.apply_action(oc);

                // TODO: Similar to above this was a call to `getChanceProb` may need to support
                // something other than just the naive uniform distribution
                ev += 1.0 / num_cos as f64
                    * self.expectimaxbr(ngs, fixed_player, opp_reach.clone(), ns);
            }

            return ev;
        }

        // declare variables and get # actions available
        let mut ev = 0.0;

        let actions = gs.legal_actions();

        let mut max_ev = f64::NEG_INFINITY;
        let mut child_evs = Vec::with_capacity(actions.len());
        let mut opp_action_dist = NormalizerMap::new();

        for i in 0..actions.len() {
            let a = actions[i];
            let mut new_opp_reach = opp_reach.clone();

            if gs.cur_player() == fixed_player {
                (opp_action_dist, new_opp_reach) = self.compute_action_dist(
                    ns,
                    &gs,
                    fixed_player,
                    opp_action_dist,
                    a,
                    new_opp_reach,
                );
            }

            // state transition + recursion
            let mut ngs = gs.clone();
            ngs.apply_action(a);
            let child_ev = self.expectimaxbr(ngs, fixed_player, new_opp_reach, ns);

            if gs.cur_player() == fixed_player {
                child_evs.push(child_ev);
            } else {
                if child_ev >= max_ev {
                    max_ev = child_ev;
                }
            }
        }

        if gs.cur_player() == fixed_player {
            opp_action_dist.normalize();
            for i in 0..actions.len() {
                // TODO: unclear what `CHKPROB` and `CHKDBL` are doing, may need other asserts
                //     CHKPROB(oppActionDist[i]);
                //     CHKDBL(childEVs[i]);
                // the child_evs are getting a neg infinity because they wouldn't be chosen, need to account for this
                if opp_action_dist[i] > 0.0 {
                    ev += opp_action_dist[i] * child_evs[i];
                }
            }
        } else {
            ev = max_ev;
        }

        return ev;
    }

    /// Compute the weight for this action over all chance outcomes
    /// Used for determining probability of action
    /// Done only at fixed_player nodes
    fn compute_action_dist<N: NodeStore>(
        &mut self,
        ns: &mut N,
        gs: &KPGameState,
        fixed_player: Player,
        mut opp_action_dist: NormalizerMap,
        action: Action,
        mut new_opp_reach: Vec<f64>,
    ) -> (NormalizerMap, Vec<f64>) {
        let player = gs.cur_player();
        assert_eq!(player, fixed_player);

        let mut weight = 0.0;

        for i in 0..self.opp_chance_outcomes.len() {
            let chance_outcome = self.opp_chance_outcomes[i];
            let key = gs.co_istate(player, chance_outcome);

            //     double oppProb = getMoveProb(is, action, actionshere);
            let node = ns.get_node_mut(&key).unwrap();
            let idx = node.get_index(action);
            let opp_prob = node.get_average_strategy()[idx];

            // TODO: figure out what CHKPROB does
            //     CHKPROB(oppProb);

            new_opp_reach[i] = new_opp_reach[i] * opp_prob as f64;

            weight += 1.0 / self.opp_chance_outcomes.len() as f64 * new_opp_reach[i]
        }

        // TODO: figure out what CHKDBL does
        //   CHKDBL(weight);

        opp_action_dist.add(action, weight);
        return (opp_action_dist, new_opp_reach);
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        cfragent::bestresponse::alwaysfirsttrainableagent::_populate_always_n,
        database::memory_node_store::MemoryNodeStore,
        kuhn_poker::{KPAction, KuhnPoker},
    };

    use super::BestResponse;

    ///  Verify that finding the optimal policy against an agent that always bets in Kuhn Poker
    ///
    /// If it is known the agent always bets, the best response for player 0 should be:
    /// * Jack: always fold
    /// * Queen: unclear
    /// * King: always bet
    #[test]
    fn test_br_always_bet_agent() {
        let g = KuhnPoker::game();
        let mut ns = MemoryNodeStore::new();
        let mut br = BestResponse::new();

        _populate_always_n(&mut ns, g, KPAction::Bet as usize);

        // best response against player 0, so as player 1
        let gs = KuhnPoker::from_actions(&[0, 1]);
        let v_0 = br.compute_best_response(gs, 1, vec![1.0, 1.0], &mut ns);
        let gs = KuhnPoker::from_actions(&[2, 1]);
        let v_2 = br.compute_best_response(gs, 1, vec![1.0, 1.0], &mut ns);

        assert_eq!(v_0, v_2); // shouldn't depend on opponents actual card, this should be normalized over the possible outcomes

        let gs = KuhnPoker::from_actions(&[1, 0]);
        let v = br.compute_best_response(gs, 1, vec![1.0, 1.0], &mut ns);
        assert_eq!(v, -1.0);

        let gs = KuhnPoker::from_actions(&[1, 2]);
        let v = br.compute_best_response(gs, 1, vec![1.0, 1.0], &mut ns);
        assert_eq!(v, 2.0);

        // With no chance outcomes decided:
        // 1/3 chance get a 0 -- should immediately fold, ev = -1
        // 1/3 chance get a 1 -- should be neutral, 50% of time win and 50% lose, ev = 0
        // 1/3 chance get a 2 -- should bet, 100% win 2
        //
        // Total should be 1/3 * (-1 + 2) = 1/3
        let gs = KuhnPoker::from_actions(&[]);
        let v = br.compute_best_response(gs, 1, vec![1.0, 1.0, 1.0], &mut ns);
        assert_eq!(v, 1.0 / 3.0); // todo: calculate what this should be

        // Can manually calculate what the exploitability will be and compare it to what comes here
        // see paper for description of calcs

        // Node 0: reach (1, 1)

        todo!();
    }
}
