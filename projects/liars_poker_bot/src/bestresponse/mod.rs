// https://aipokertutorial.com/agent-evaluation/

mod alwaysfirsttrainableagent;
mod normalizer;

use std::{collections::HashMap, ops::Index};

use log::debug;

use crate::{
    actions,
    bestresponse::normalizer::{NormalizerMap, NormalizerVector},
    cfragent::cfrnode::CFRNode,
    collections::ArrayVec,
    database::NodeStore,
    game::{Action, Game, GameState, Player},
};

#[derive(Copy, Clone)]
pub struct ChanceOutcome {
    data: ArrayVec<5>,
}

impl ChanceOutcome {
    pub fn new(actions: Vec<Action>) -> Self {
        let mut data = ArrayVec::new();
        for a in actions {
            data.push(a)
        }

        return Self { data: data };
    }

    pub fn len(&self) -> usize {
        return self.data.len();
    }
}

impl Index<usize> for ChanceOutcome {
    type Output = Action;

    fn index(&self, index: usize) -> &Self::Output {
        return &self.data[index];
    }
}

pub struct BestResponse {
    /// Vector of possible private chance outcomes for a given game. For example
    /// in KuhnPoker, this would be the dealt cards [[0], [1], [2]]. In Euchre, this would be all
    /// possible hand states [[0, 1, 2, 3, 4], [0, 1, 2, 3, 5],  ...]
    ///
    /// For now we're ignoring how to handle the discard card in Euchre.
    opp_chance_outcomes: Vec<ChanceOutcome>,
}

impl BestResponse {
    pub fn new() -> Self {
        Self {
            opp_chance_outcomes: Vec::new(),
        }
    }

    /// Get exploitability by iterating over all possible chance outcomes
    pub fn get_exploitability<T: NodeStore<CFRNode>, G: GameState>(
        &mut self,
        g: &Game<G>,
        ns: &mut T,
        fixed_player: Player,
    ) -> f64 {
        let mut value_sum = 0.0;
        let mut n = 0;

        let mut chance_nodes = Vec::new();
        let mut play_nodes = HashMap::new();

        let ogs = (&g.new)();
        assert!(ogs.is_chance_node());
        chance_nodes.push(ogs.clone());
        let nf = (fixed_player + 1) % 2;

        // collect all the chance nodes
        let default = &(ogs, 0);
        while let Some(gs) = chance_nodes.pop() {
            for a in actions!(gs) {
                let mut ngs = gs.clone();
                ngs.apply_action(a);
                if ngs.is_chance_node() {
                    chance_nodes.push(ngs);
                } else {
                    let non_fixed_key = ngs.istate_key(nf);
                    // We only need to evaluate the unique nodes for the non_fixed player, so we just weight the outcomes
                    let (_, count) = play_nodes.get(&non_fixed_key).unwrap_or(default);
                    play_nodes.insert(non_fixed_key, (ngs, count + 1));
                }
            }
        }
        debug!("found {} starting nodes to evaluate", play_nodes.len());
        let mut remaining_nodes = play_nodes.len();
        for (gs, count) in play_nodes.values() {
            let v = self.compute_best_response(gs.clone(), fixed_player, ns);
            value_sum += v * *count as f64;
            n += count;
            remaining_nodes -= 1;
            debug!("nodes remaining: {}", remaining_nodes);
        }

        return value_sum / n as f64;
    }

    /// Runs the best response algorithm
    ///
    /// The `fixed_player` is the player using the stored policy from `ns`
    pub fn compute_best_response<T: NodeStore<CFRNode>, G: GameState>(
        &mut self,
        gs: G,
        fixed_player: Player,
        ns: &mut T,
    ) -> f64 {
        self.opp_chance_outcomes = gs.chance_outcomes(fixed_player);
        return self.expectimaxbr(
            gs,
            fixed_player,
            vec![1.0; self.opp_chance_outcomes.len()],
            ns,
        );
    }

    /// Implements the best response alogirhtm from Marc's thesis.
    ///
    /// Args:
    ///     gs: Gamestate
    ///     opp_reach: chance of reaching this istate given the corresponsding opp chance outcomes
    ///     fixed_player: player with the policy in the node store
    ///     ns: node store
    pub fn expectimaxbr<T: NodeStore<CFRNode>, G: GameState>(
        &mut self,
        gs: G,
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
            panic!(
                "don't yet support chance nodes, set all chance outcomes before calling expectimax"
            );
        }

        // declare variables and get # actions available
        let mut ev = 0.0;

        let actions = actions!(gs);

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
                let a = actions[i];
                // TODO: unclear what `CHKPROB` and `CHKDBL` are doing, may need other asserts
                //     CHKPROB(oppActionDist[i]);
                //     CHKDBL(childEVs[i]);
                // the child_evs are getting a neg infinity because they wouldn't be chosen, need to account for this
                if opp_action_dist[a] > 0.0 {
                    ev += opp_action_dist[a] * child_evs[i];
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
    fn compute_action_dist<N: NodeStore<CFRNode>, G: GameState>(
        &mut self,
        ns: &mut N,
        gs: &G,
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
            let node = ns.get(&key);
            let opp_prob;
            if node.is_none() {
                opp_prob = 1.0 / actions!(gs).len() as f64;
            } else {
                let node = node.unwrap();
                opp_prob = node.borrow().get_average_strategy()[action];
            }

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
        bestresponse::alwaysfirsttrainableagent::_populate_always_n,
        database::memory_node_store::MemoryNodeStore,
        game::kuhn_poker::{KPAction, KuhnPoker},
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

        _populate_always_n(&mut ns, &g, KPAction::Bet as usize);

        // best response against player 0, so as player 1
        let gs = KuhnPoker::from_actions(&[KPAction::Jack, KPAction::Queen]);
        let v_0 = br.compute_best_response(gs, 0, &mut ns);
        let gs = KuhnPoker::from_actions(&[KPAction::King, KPAction::Queen]);
        let v_2 = br.compute_best_response(gs, 0, &mut ns);

        assert_eq!(v_0, v_2); // shouldn't depend on opponents actual card, this should be normalized over the possible outcomes

        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::Jack]);
        let v = br.compute_best_response(gs, 0, &mut ns);
        assert_eq!(v, -1.0);

        let gs = KuhnPoker::from_actions(&[KPAction::Queen, KPAction::King]);
        let v = br.compute_best_response(gs, 0, &mut ns);
        assert_eq!(v, 2.0);

        // With no chance outcomes decided:
        // 1/3 chance get a 0 -- should immediately fold, ev = -1
        // 1/3 chance get a 1 -- should be neutral, 50% of time win and 50% lose, ev = 0
        // 1/3 chance get a 2 -- should bet, 100% win 2
        //
        // Total should be 1/3 * (0 + -1 + 2) = 1/3

        let avg = br.get_exploitability(&g, &mut ns, 0);
        assert_eq!(avg, 1.0 / 3.0);

        // same for player 1
        let avg = br.get_exploitability(&g, &mut ns, 1);
        assert_eq!(avg, 1.0 / 3.0);
    }
}
