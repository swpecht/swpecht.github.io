use std::{
    collections::HashMap,
    fs::{self, File},
    io::BufWriter,
};

use log::info;
use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};
use rmp_serde::Serializer;
use serde::{Deserialize, Serialize};

use crate::{
    algorithms::{
        ismcts::{Evaluator, ResampleFromInfoState},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    alloc::Pool,
    game::{
        euchre::{processors::post_bidding_phase, EuchreGameState},
        Action, GameState, Player,
    },
    istate::IStateKey,
    metrics::increment_counter,
    policy::Policy,
};

use super::cfrnode::ActionVec;

enum AverageType {
    _Full,
    Simple,
}

#[derive(Serialize, Deserialize)]
struct InfoState {
    regrets: ActionVec<f64>,
    avg_strategy: ActionVec<f64>,
}

impl InfoState {
    pub(super) fn new(actions: &Vec<Action>) -> Self {
        let mut regrets = ActionVec::new(actions);
        let mut avg_strategy = ActionVec::new(actions);

        // Start with a small amount of regret and total accumulation, to give a
        // uniform policy: this will get erased fast.
        for a in actions {
            regrets[*a] = 1.0 / 1e6;
            avg_strategy[*a] = 1.0 / 1e6;
        }

        Self {
            regrets,
            avg_strategy,
        }
    }
}

/// Implementation of external sampled CFR
///
/// Based on implementation from: OpenSpiel:
///     https://github.com/deepmind/open_spiel/blob/master/open_spiel/python/algorithms/mccfr.py
pub struct CFRES<G> {
    rng: StdRng,
    vector_pool: Pool<Vec<Action>>,
    game_generator: fn() -> G,
    average_type: AverageType,
    infostates: HashMap<IStateKey, InfoState>,
    /// determine if we are at the max depth and should use the rollout
    is_max_depth: fn(&G) -> bool,
    evaluator: PIMCTSBot<G, OpenHandSolver<G>>,
}

impl CFRES<EuchreGameState> {
    pub fn new_euchre_bidding(game_generator: fn() -> EuchreGameState, mut rng: StdRng) -> Self {
        let pimcts_seed = rng.gen();
        Self {
            rng,
            vector_pool: Pool::new(Vec::new),
            game_generator,
            average_type: AverageType::Simple,
            infostates: HashMap::new(),
            is_max_depth: post_bidding_phase,
            evaluator: PIMCTSBot::new(
                50,
                OpenHandSolver::new_euchre(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
        }
    }
}

impl<G: GameState + ResampleFromInfoState> CFRES<G> {
    pub fn new(game_generator: fn() -> G, mut rng: StdRng) -> Self {
        let pimcts_seed = rng.gen();
        Self {
            rng,
            vector_pool: Pool::new(Vec::new),
            game_generator,
            average_type: AverageType::Simple,
            infostates: HashMap::new(),
            is_max_depth: |_: &G| false,
            evaluator: PIMCTSBot::new(
                50,
                OpenHandSolver::default(),
                SeedableRng::seed_from_u64(pimcts_seed),
            ),
        }
    }

    pub fn train(&mut self, n: usize) {
        for _ in 0..n {
            self.iteration();
        }
        self.evaluator.reset();
    }

    pub fn save(&self) {
        fs::rename("/tmp/infostates", "/tmp/infostates.old")
            .expect("error backing up previous file");
        let f = File::create("/tmp/infostates").unwrap();
        let f = BufWriter::new(f);

        info!("saving weights for {} infostates...", self.infostates.len());
        self.infostates.serialize(&mut Serializer::new(f)).unwrap();
    }

    pub fn load(&mut self) {
        let f = &mut File::open("/tmp/infostates");
        let f = f.as_mut().unwrap();
        self.infostates = rmp_serde::from_read(f).unwrap();
        info!("loaded weights for {} infostates", self.infostates.len());
    }

    /// erforms one iteration of external sampling.
    ///
    /// An iteration consists of one episode for each player as the update
    /// player.
    fn iteration(&mut self) {
        let num_players = (self.game_generator)().num_players();
        for player in 0..num_players {
            self.update_regrets(&mut (self.game_generator)(), player, 0);
        }

        if matches!(self.average_type, AverageType::_Full) {
            let reach_probs = vec![1.0; num_players];
            self.full_update_average((self.game_generator)(), reach_probs);
        }
    }

    /// Runs an episode of external sampling.
    ///
    /// Args:
    ///     state: the game state to run from
    ///     player: the player to update regrets for
    ///
    /// Returns:
    ///     value: is the value of the state in the game
    ///     obtained as the weighted average of the values
    ///     of the children
    fn update_regrets(&mut self, gs: &mut G, player: Player, _depth: usize) -> f64 {
        if gs.is_terminal() {
            return gs.evaluate(player);
        }

        if gs.is_chance_node() {
            let mut actions = self.vector_pool.detach();
            gs.legal_actions(&mut actions);
            let outcome = *actions
                .choose(&mut self.rng)
                .expect("error choosing a random action for chance node");
            actions.clear();
            self.vector_pool.attach(actions);

            gs.apply_action(outcome);
            let value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
            return value;
        }

        // If we're at max depth, do the rollout
        if (self.is_max_depth)(gs) {
            return self.evaluator.evaluate_player(gs, player);
        }

        increment_counter("cfr.cfres.nodes_touched");

        let cur_player = gs.cur_player();
        let info_state_key = gs.istate_key(cur_player);
        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);

        let policy;
        {
            let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
            policy = regret_matching(&infostate_info.regrets);
        }

        let mut value = 0.0;
        let mut child_values = ActionVec::new(&actions);

        if cur_player != player {
            // sample at opponent node
            let a = policy
                .to_vec()
                .choose_weighted(&mut self.rng, |a| a.1)
                .expect("error choosing weighted action")
                .0;
            gs.apply_action(a);
            value = self.update_regrets(gs, player, _depth + 1);
            gs.undo();
        } else {
            // walk over all actions at my node
            for &a in actions.iter() {
                gs.apply_action(a);
                child_values[a] = self.update_regrets(gs, player, _depth + 1);
                gs.undo();
                value += policy[a] * child_values[a];
            }
        }

        if cur_player == player {
            // update regrets
            let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
            for &a in actions.iter() {
                add_regret(infostate_info, a, child_values[a] - value);
            }
        }

        // Simple average does averaging on the opponent node. To do this in a game
        // with more than two players, we only update the player + 1 mod num_players,
        // which reduces to the standard rule in 2 players.
        //
        // We adapt this slightly for euchre where it alternates what team the players are on
        let cur_team = cur_player % 2;
        let player_team = player % 2;
        if matches!(self.average_type, AverageType::Simple) && cur_team != player_team {
            let infostate_info = self.lookup_infostate_info(&info_state_key, &actions);
            for &action in actions.iter() {
                add_avstrat(infostate_info, action, policy[action]);
            }
        }

        actions.clear();
        self.vector_pool.attach(actions);

        value
    }

    /// Looks up an information set table for the given key.
    fn lookup_infostate_info(&mut self, key: &IStateKey, actions: &Vec<Action>) -> &mut InfoState {
        self.infostates
            .entry(*key)
            .or_insert(InfoState::new(actions))
    }

    fn full_update_average(&mut self, _gs: G, _reach_probs: Vec<f64>) {
        todo!();
    }

    pub fn num_info_states(&self) -> usize {
        self.infostates.len()
    }
}

/// Applies regret matching to get a policy.
///
/// Args:
///   regrets: vector regrets for each action.
///
/// Returns:
///   probability of taking each action
fn regret_matching(regrets: &ActionVec<f64>) -> ActionVec<f64> {
    let sum_pos_regrets: f64 = regrets.to_vec().iter().map(|(_, b)| b.max(0.0)).sum();
    let mut policy = ActionVec::new(regrets.actions());

    if sum_pos_regrets <= 0.0 {
        for a in regrets.actions() {
            policy[*a] = 1.0 / regrets.actions().len() as f64;
        }
    } else {
        for a in regrets.actions() {
            policy[*a] = regrets[*a].max(0.0) / sum_pos_regrets;
        }
    }

    policy
}

fn add_regret(infostate: &mut InfoState, action: Action, amount: f64) {
    infostate.regrets[action] += amount;
}

fn add_avstrat(infostate: &mut InfoState, action: Action, amount: f64) {
    infostate.avg_strategy[action] += amount;
}

impl<G: GameState + ResampleFromInfoState + Send> Policy<G> for CFRES<G> {
    /// Returns the MCCFR average policy for a player in a state.
    ///
    /// If the policy is not defined for the provided state, a uniform
    /// random policy is returned.
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let player = gs.cur_player();

        if (self.is_max_depth)(gs) {
            return self.evaluator.action_probabilities(gs);
        }

        let mut actions = self.vector_pool.detach();
        gs.legal_actions(&mut actions);
        let info_state_key = gs.istate_key(player);
        let retrieved_infostate = self.infostates.get(&info_state_key);

        let mut policy = ActionVec::new(&actions);
        if let Some(retrieved_infostate) = retrieved_infostate {
            let policy_sum: f64 = retrieved_infostate
                .avg_strategy
                .to_vec()
                .iter()
                .map(|(_, v)| *v)
                .sum();
            for a in actions.iter() {
                policy[*a] = retrieved_infostate.avg_strategy[*a] / policy_sum;
            }
        } else {
            for a in actions.iter() {
                policy[*a] = 1.0 / actions.len() as f64;
            }
        }

        self.vector_pool.attach(actions);

        policy
    }
}

#[cfg(test)]
mod tests {

    use rand::SeedableRng;

    use crate::{cfragent::cfres::CFRES, game::kuhn_poker::KuhnPoker};

    #[test]
    fn cfres_train_test() {
        let mut alg = CFRES::new(|| (KuhnPoker::game().new)(), SeedableRng::seed_from_u64(43));
        alg.train(10);
    }
}
