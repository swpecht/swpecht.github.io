use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
};

use log::{debug, trace};
use rand::{rngs::StdRng, seq::SliceRandom, thread_rng, SeedableRng};
use rayon::prelude::*;

use crate::{
    actions,
    agents::Agent,
    algorithms::alphamu::front::AMVector,
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    istate::IStateKey,
    policy::{self, Policy},
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

/// Implementation for AlphaMu from "The αµ Search Algorithm for the Game of Bridge"
///
/// https://arxiv.org/pdf/1911.07960.pdf
pub struct AlphaMuBot<G, E> {
    evaluator: E,
    cache: AlphaMuCache<G>,
    team: Team,
    num_worlds: usize,
    m: usize,
    _phantom_data: PhantomData<G>,
    rng: StdRng,
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> AlphaMuBot<G, E> {
    pub fn new(evaluator: E, num_worlds: usize, m: usize) -> Self {
        if m < 1 {
            panic!("m must be at least 1, m=1 is PIMCTS")
        }

        Self {
            evaluator,
            team: Team::Team1,
            _phantom_data: PhantomData,
            num_worlds,
            m,
            cache: AlphaMuCache::default(),
            rng: SeedableRng::seed_from_u64(42),
        }
    }

    pub fn run_search(&mut self, root_node: &G) -> ActionVec<f64> {
        self.reset();

        let root_node = root_node.clone();
        let mut rng = thread_rng();
        let player = root_node.cur_player();
        self.team = match player {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        };

        let mut worlds = Vec::new();
        for _ in 0..self.num_worlds {
            worlds.push(Some(root_node.resample_from_istate(player, &mut rng)))
        }

        let actions = actions!(root_node);
        let mut policy = ActionVec::new(&actions);

        // todo, implement other methods for choosing action than just best option
        let mut max_wins = 0.0;
        let mut max_action = actions[0];
        for a in actions {
            let a_worlds = self.filter_and_progress_worlds(&worlds, a);
            let front = self.alphamu(self.m - 1, a_worlds.clone(), None);
            let wins = front.avg_wins();
            debug!(
                "evaluated {} to avg wins of {} with front: {:?}",
                a_worlds
                    .iter()
                    .flatten()
                    .next()
                    .unwrap()
                    .istate_string(player),
                wins,
                front
            );
            if wins > max_wins {
                max_action = a;
                max_wins = wins;
            }
        }

        policy[max_action] = 1.0;
        policy
    }

    /// Runs alphamu search returning the new front and optionally the actions to achieve it
    fn alphamu(&mut self, m: usize, worlds: Vec<Option<G>>, alpha: Option<AMFront>) -> AMFront {
        let w = worlds.iter().flatten().next().unwrap();
        trace!(
            "alpha mu call: istate: {}\tm: {}",
            w.istate_string(self.team.into()),
            m
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
            return result;
        }

        let cached = self.cache.get(&worlds, self.team);
        if let Some(c) = cached {
            return c.clone();
        }

        let mut front = AMFront::default();
        if self.team != self.get_team(&worlds) {
            // min node
            for a in self.all_moves(&worlds) {
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);

                let f = self.alphamu(m, worlds_1, None);
                front = front.min(f);
            }
        } else {
            // max node
            for a in self.all_moves(&worlds) {
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                // let f = self.alphamu(m - 1, worlds_1, Some(front.clone()));
                let f = self.alphamu(m - 1, worlds_1, None);
                front = front.max(f);
            }
        }

        assert!(!front.is_empty());

        trace!(
            "alpha mu found: istate: {}\tm: {}\t{:?}",
            w.istate_string(self.team.into()),
            m,
            front
        );
        self.cache.insert(&worlds, front.clone(), self.team);
        self.cache.world_vector_pool.attach(worlds);
        front
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

    /// Returns the progressed worlds where `a` was a valid action. Otherwise it
    /// marks that spot as None
    fn filter_and_progress_worlds(&mut self, worlds: &Vec<Option<G>>, a: Action) -> Vec<Option<G>> {
        let mut worlds_1 = self.cache.world_vector_pool.detach();
        worlds_1.clear();
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
                let v = self.evaluator.evaluate_player(w, self.team.into());
                if v > 0.0 {
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

    fn reset(&mut self) {}

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

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Policy<G> for AlphaMuBot<G, E> {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        self.run_search(gs)
    }
}

struct AlphaMuCache<G> {
    world_vector_pool: Pool<Vec<Option<G>>>,
    transposition_table: HashMap<(Team, IStateKey), AMFront>,
}

impl<G> Default for AlphaMuCache<G> {
    fn default() -> Self {
        Self {
            world_vector_pool: Pool::new(Vec::new),
            transposition_table: HashMap::default(),
        }
    }
}

impl<G: GameState> AlphaMuCache<G> {
    fn get(&mut self, _worlds: &[Option<G>], _max_team: Team) -> Option<&AMFront> {
        // let key = worlds
        //     .iter()
        //     .flatten()
        //     .next()
        //     .unwrap()
        //     .istate_key(max_team.into());

        // self.transposition_table.get(&(max_team, key))
        None
    }

    pub fn insert(&mut self, _worlds: &[Option<G>], _v: AMFront, _maximizing_team: Team) {

        // // Check if the game wants to store this state
        // let key = worlds
        //     .iter()
        //     .flatten()
        //     .next()
        //     .unwrap()
        //     .istate_key(maximizing_team.into());
        // self.transposition_table.insert((maximizing_team, key), v);
    }
}

#[cfg(test)]
mod tests {
    use rand::{seq::SliceRandom, thread_rng};

    use crate::{
        actions,
        agents::{Agent, RandomAgent},
        algorithms::ismcts::RandomRolloutEvaluator,
        game::{kuhn_poker::KuhnPoker, GameState},
    };

    use super::AlphaMuBot;

    #[test]
    fn test_alpha_mu_is_agent() {
        let mut alphamu = AlphaMuBot::new(RandomRolloutEvaluator::new(100), 20, 10);
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
