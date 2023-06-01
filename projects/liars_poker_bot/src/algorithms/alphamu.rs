use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

use log::{debug, trace};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};
use rustc_hash::FxHashMap;

use crate::{
    actions,
    agents::Agent,
    algorithms::alphamu::front::AMVector,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    istate::IStateKey,
};

use self::front::AMFront;

use super::ismcts::{Evaluator, ResampleFromInfoState};

mod front;

#[derive(PartialEq, Eq, Clone, Copy, Hash)]
pub enum Team {
    Team1,
    Team2,
}

impl From<Team> for Player {
    fn from(val: Team) -> Self {
        val as usize
    }
}

impl From<Player> for Team {
    fn from(val: Player) -> Self {
        if val % 2 == 0 {
            Team::Team1
        } else {
            Team::Team2
        }
    }
}

#[derive(Default)]
struct ChildNode {
    /// Average chance of winning averaged across all worlds in all fronts
    win_sum: FxHashMap<Action, f64>,
}

impl ChildNode {
    fn update_action_value(&mut self, a: Action, f: &AMFront) {
        let sum = self.win_sum.entry(a).or_insert(0.0);
        *sum = f.avg_wins();
    }
}

/// Implementation for AlphaMu from "The αµ Search Algorithm for the Game of Bridge"
///
/// https://arxiv.org/pdf/1911.07960.pdf
pub struct AlphaMuBot<G, E> {
    evaluator: E,
    team: Team,
    num_worlds: usize,
    m: usize,
    _phantom_data: PhantomData<G>,
    nodes: HashMap<IStateKey, ChildNode>,
    rng: StdRng,
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> AlphaMuBot<G, E> {
    pub fn new(evaluator: E, num_worlds: usize, m: usize) -> Self {
        Self {
            evaluator,
            team: Team::Team1,
            _phantom_data: PhantomData,
            num_worlds,
            m,
            nodes: HashMap::default(),
            rng: SeedableRng::seed_from_u64(42),
        }
    }

    pub fn run_search(&mut self, root_node: &G) -> ActionVec<f64> {
        self.reset();

        let mut rng = thread_rng();
        self.team = match root_node.cur_player() {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        };
        let player = root_node.cur_player();
        let mut worlds = Vec::new();
        for _ in 0..self.num_worlds {
            worlds.push(Some(root_node.resample_from_istate(player, &mut rng)))
        }

        self.alphamu(self.m, worlds);
        self.get_final_policy(root_node)
    }

    /// Returns the policy normalized to the win rates per action
    fn get_final_policy(&self, root_node: &G) -> ActionVec<f64> {
        let key = root_node.istate_key(root_node.cur_player());
        let node = self.nodes.get(&key).unwrap();

        let v_sum: f64 = node.win_sum.values().sum();
        let actions = actions!(root_node);
        let mut policy = ActionVec::new(&actions);

        if v_sum == 0.0 {
            // no wins to play randomly
            let prob = 1.0 / actions.len() as f64;
            for a in actions {
                policy[a] = prob;
            }
            return policy;
        }

        for (a, v) in &node.win_sum {
            policy[*a] = v / v_sum;
        }

        policy
    }

    fn alphamu(&mut self, m: usize, worlds: Vec<Option<G>>) -> AMFront {
        trace!(
            "alpha mu call: istate: {:?}\tm: {}\tworlds: {:?}",
            self.cur_istate(&worlds),
            m,
            worlds
        );

        // ensure we have no chance nodes, not yet implemented
        worlds
            .iter()
            .flatten()
            .map(|w| assert!(!w.is_chance_node()))
            .count();

        let mut result = AMFront::default();
        result.push(AMVector::from_worlds(&worlds));
        if self.stop(m, &worlds, &mut result) {
            trace!(
                "stopping alpha mu for {:?}, found front: {:?}",
                self.cur_istate(&worlds),
                result
            );
            return result;
        }

        let mut front = AMFront::default();
        if self.team != self.get_team(&worlds) {
            // min node
            for a in self.all_moves(&worlds) {
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let key = self.cur_istate(&worlds_1);
                let f = self.alphamu(m, worlds_1);
                self.save_node(key, a, &f);
                front = front.min(f);
            }
        } else {
            // max node
            for a in self.all_moves(&worlds) {
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let key = self.cur_istate(&worlds_1);
                let f = self.alphamu(m - 1, worlds_1);
                self.save_node(key, a, &f);
                front = front.max(f);
            }
        }

        assert!(!front.is_empty());
        front
    }

    fn save_node(&mut self, key: IStateKey, a: Action, f: &AMFront) {
        let node = self.nodes.entry(key).or_insert(ChildNode::default());
        node.update_action_value(a, f);
    }

    fn all_moves(&self, worlds: &[Option<G>]) -> HashSet<Action> {
        let mut all_moves = HashSet::new();
        let mut actions = Vec::new();
        for w in worlds.iter().flatten() {
            w.legal_actions(&mut actions);
            for a in &actions {
                all_moves.insert(*a);
            }
        }
        all_moves
    }

    /// Returns the istate key for all valid worlds
    fn cur_istate(&self, worlds: &[Option<G>]) -> IStateKey {
        let p = worlds
            .iter()
            .flatten()
            .map(|x| x.cur_player())
            .next()
            .unwrap();

        worlds
            .iter()
            .flatten()
            .map(|w| w.istate_key(p))
            .next()
            .expect("no valid worlds for istate")
    }

    /// Returns the progressed worlds where `a` was a valid action. Otherwise it
    /// marks that spot as None
    fn filter_and_progress_worlds(&mut self, worlds: &Vec<Option<G>>, a: Action) -> Vec<Option<G>> {
        let mut worlds_1 = Vec::with_capacity(worlds.len());
        let mut actions = Vec::new();
        for w in worlds.iter() {
            if w.is_none() {
                worlds_1.push(None);
                continue;
            }
            let w = w.as_ref().unwrap();
            w.legal_actions(&mut actions);
            if actions.contains(&a) {
                let nw = self.play(w, a);
                worlds_1.push(Some(nw));
            } else {
                worlds_1.push(None)
            }
        }

        assert_eq!(worlds.len(), worlds_1.len());
        worlds_1
    }

    fn play(&mut self, gs: &G, a: Action) -> G {
        let mut ngs = gs.clone();
        ngs.apply_action(a);
        ngs
    }

    fn stop(&mut self, m: usize, worlds: &[Option<G>], result: &mut AMFront) -> bool {
        let mut all_states_terminal = true;
        for w in worlds.iter().flatten() {
            all_states_terminal &= w.is_terminal();
        }

        let mut any_states_terminal = false;
        for w in worlds.iter().flatten() {
            any_states_terminal |= w.is_terminal();
        }
        if all_states_terminal != any_states_terminal {
            panic!("worlds not all terminating at same time: {:?}", worlds)
        }

        if any_states_terminal {
            // Unlike in bridge, we can't determin who won only from the public information at the gamestate
            // instead we need to evaluate each world individually
            for (i, w) in worlds.iter().enumerate() {
                if w.is_none() {
                    continue;
                }
                let w = w.as_ref().unwrap();
                let v = w.evaluate(self.team.into());
                if v > 0.0 {
                    result.set(i, true);
                } else {
                    result.set(i, false);
                }
            }
            return true;
        }

        if m == 0 {
            for (i, w) in worlds.iter().enumerate().filter(|(_, w)| w.is_some()) {
                let w = w.as_ref().unwrap();
                let r = self.evaluator.evaluate(w);
                if r[self.team as usize] > 0.0 {
                    // win most times
                    result.set(i, true);
                } else {
                    result.set(i, false);
                }
            }
            return true;
        }

        false
    }

    fn reset(&mut self) {
        self.nodes.clear();
    }

    fn get_team(&self, worlds: &[Option<G>]) -> Team {
        let player = worlds
            .iter()
            .flatten()
            .map(|x| x.cur_player())
            .next()
            .unwrap();

        match player {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        }
    }
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Agent<G> for AlphaMuBot<G, E> {
    fn step(&mut self, s: &G) -> Action {
        let action_weights = self.run_search(s).to_vec();
        action_weights
            .choose_weighted(&mut self.rng, |item| item.1)
            .unwrap()
            .0
    }

    fn get_name(&self) -> String {
        format!(
            "AlphaMu, worlds: {}, m: {}, evaluator: todo",
            self.num_worlds, self.m
        )
    }
}

#[cfg(test)]
mod tests {
    use rand::{seq::SliceRandom, thread_rng, SeedableRng};

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::ismcts::RandomRolloutEvaluator,
        game::{kuhn_poker::KuhnPoker, GameState},
    };

    use super::AlphaMuBot;

    #[test]
    fn test_alpha_mu_is_agent() {
        let rng = SeedableRng::seed_from_u64(42);
        let mut alphamu = AlphaMuBot::new(RandomRolloutEvaluator::new(100, rng), 20, 10);
        let mut opponent = RandomAgent::default();

        for _ in 0..10 {
            let mut gs = KuhnPoker::new_state();
            while gs.is_chance_node() {
                let a = *actions!(gs).choose(&mut thread_rng()).unwrap();
                gs.apply_action(a)
            }

            while !gs.is_terminal() {
                let a = match gs.cur_player() {
                    0 => alphamu.step(&gs),
                    1 => opponent.step(&gs),
                    _ => panic!("invalid player"),
                };
                gs.apply_action(a);
            }

            gs.evaluate(0);
        }
    }
}
