use std::{
    collections::{hash_map::DefaultHasher, HashMap, HashSet},
    fmt::Display,
    hash::{Hash, Hasher},
};

use itertools::Itertools;
use log::trace;
use rand::rngs::StdRng;

use crate::{
    actions,
    algorithms::{alphamu::front::AMVector, pimcts::get_worlds},
    alloc::Pool,
    cfragent::cfrnode::ActionVec,
    game::{Action, GameState, Player},
    policy::Policy,
};

use self::front::{AMFront, VectorValue};

use super::ismcts::{Evaluator, ResampleFromInfoState};

mod front;

const USELESS_WORLD_VALUE: VectorValue = VectorValue::BigLoss;

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

#[derive(Debug, Clone, Copy)]
enum WorldState<G> {
    Useful(G),
    Useless,
    Invalid,
}

impl<G: Display> Display for WorldState<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WorldState::Useful(gs) => write!(f, "{}", gs),
            WorldState::Useless => write!(f, "Useless"),
            WorldState::Invalid => write!(f, "Invalid"),
        }
    }
}

impl<G> WorldState<G> {
    pub fn is_useful(&self) -> bool {
        matches!(self, WorldState::Useful(_))
    }

    pub fn is_invalid(&self) -> bool {
        matches!(self, WorldState::Invalid)
    }

    pub fn is_useless(&self) -> bool {
        matches!(self, WorldState::Useless)
    }

    pub fn unwrap(&self) -> &G {
        match self {
            WorldState::Useful(gs) => gs,
            WorldState::Useless | WorldState::Invalid => panic!("called unwrap on invalid world"),
        }
    }
}

impl<'a, G> IntoIterator for &'a WorldState<G> {
    type Item = &'a G;
    type IntoIter = <Option<&'a G> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        use WorldState::*;
        match self {
            Useful(g) => Some(g),
            Useless | Invalid => None,
        }
        .into_iter()
    }
}

/// Implementation for AlphaMu from "The αµ Search Algorithm for the Game of Bridge"
///
/// https://arxiv.org/pdf/1911.07960.pdf
pub struct AlphaMuBot<G, E> {
    pub use_optimizations: bool,
    evaluator: E,
    cache: AlphaMuCache<G>,
    team: Team,
    num_worlds: usize,
    m: usize,
    rng: StdRng,
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> AlphaMuBot<G, E> {
    pub fn new(evaluator: E, num_worlds: usize, m: usize, rng: StdRng) -> Self {
        if m < 1 {
            panic!("m must be at least 1, m=1 is PIMCTS")
        }

        Self {
            use_optimizations: true,
            evaluator,
            team: Team::Team1,
            num_worlds,
            m,
            cache: AlphaMuCache::default(),
            rng,
        }
    }

    pub fn run_search(
        &mut self,
        root_node: &G,
        maximizing_player: Player,
    ) -> (f64, Option<Action>) {
        // Since the cache is only keyed off the move history, we need to reset between every call
        self.reset();

        let player = maximizing_player;
        self.team = match player {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        };

        let worlds = get_worlds(root_node, self.num_worlds, &mut self.rng);
        let worlds = worlds
            .into_iter()
            .map(|w| WorldState::Useful(w))
            .collect_vec();

        trace!("running search with wolrds: {}", get_worlds_string(&worlds));

        let mut s = Vec::new();

        // Iterative deepening
        // For now this is commented out as it doesn't seem to improve performance
        if self.use_optimizations {
            for i in 1..self.m {
                trace!("starting alpha mu iterative deepening with m={}", i);
                self.alphamu(&mut s, i, worlds.clone(), None);
            }
        }
        let (front, a) = self.alphamu(&mut s, self.m, worlds, None);

        (front.score(), a)
    }

    /// Runs alphamu search returning the new front and optionally the actions to achieve it
    fn alphamu(
        &mut self,
        s: &mut Vec<Action>,
        m: usize,
        mut worlds: Vec<WorldState<G>>,
        alpha: Option<AMFront>,
    ) -> (AMFront, Option<Action>) {
        trace!("alpha mu call for: s={:?}, m={}", s, m);

        // ensure we have no chance nodes, not yet implemented
        worlds
            .iter()
            .flatten()
            .map(|w| assert!(!w.is_chance_node()))
            .count();

        {
            let mut result = AMFront::new(AMVector::from_worlds(&worlds));
            if self.stop(m, &worlds, &mut result) {
                assert!(!result.is_empty());
                if self.use_optimizations {
                    let value = TableValue {
                        front: result.clone(),
                        action: None,
                        is_max_node: false,
                    };
                    self.cache.insert(s, value);
                }
                return (result, None);
            }
        }

        let mut front = AMFront::default();
        let mut best_action = None;
        let is_max_node = self.team == self.get_team(&worlds);

        let table_value = self.cache.get(s);

        if !is_max_node {
            // min node
            let mut min_score = f64::INFINITY;

            if table_value.is_some()
                && self.use_optimizations
                && alpha.is_some()
                && table_value
                    .unwrap()
                    .front
                    .less_than_or_equal(&alpha.clone().unwrap())
            {
                trace!(
                    "early min cut: s: {:?}, t: {:?}, alpha: {:?}",
                    s,
                    table_value.unwrap().front,
                    &alpha.unwrap()
                );
                return (front, None);
            }

            if let Some(t) = table_value {
                self.update_useful_worlds(&t.front, &mut worlds);
            }

            let moves: Vec<Action> = self.all_moves(&worlds, &table_value);

            for a in moves {
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                s.push(a);
                let (f, _) = self.alphamu(s, m, worlds_1, None);
                s.pop();

                if f.score() < min_score {
                    min_score = f.score();
                    best_action = Some(a);
                }

                front = front.min(f);

                // The results for this min node will only get worse as we search more
                // If this node is already  dominated by an upper max node, we'll never choose the
                // results of this search. So we can cut the search early
                if self.is_dominated_by_upper_max(s, &front) {
                    break;
                }

                self.update_useful_worlds(&front, &mut worlds);
                // debug!("iterating on min nodes, front size: {}: {}", m, front.len());

                // set everything in the front to be useless
                for (i, _) in worlds.iter().enumerate().filter(|(_, w)| w.is_useless()) {
                    front.set(i, USELESS_WORLD_VALUE);
                }
            }

            // If the front is empty (due to useless worlds, set it to a default one)
            // in the next step, we'll populate all values
            if front.is_empty() {
                // We should only be here if there are no useless worlds
                assert_eq!(worlds.iter().filter(|w| w.is_useful()).count(), 0);
                front = AMFront::new(AMVector::from_worlds(&worlds));
                for (i, _) in worlds.iter().enumerate().filter(|(_, w)| w.is_useless()) {
                    front.set(i, USELESS_WORLD_VALUE);
                }
            }
        } else {
            // max node
            let mut max_score = f64::NEG_INFINITY;

            let moves: Vec<Action> = self.all_moves(&worlds, &table_value);
            let table_score = table_value.map(|t| t.front.score());

            for a in moves {
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                s.push(a);
                let (f, _) = self.alphamu(s, m - 1, worlds_1, Some(front.clone()));
                s.pop();

                // Could have an empty front returned by a min node where we do an alpha cut
                if !f.is_empty() && f.score() > max_score {
                    max_score = f.score();
                    best_action = Some(a);
                }

                front = front.max(f);

                // Temporarily disabling this as deeper searchers can end up being better with impossible moves
                // // Root cut optimization from the optimizing alpha mu paper:
                // //
                // // If a move at the root of αµ for M Max moves gives the same proba-
                // // bility of winning than the best move of the previous iteration of iter-
                // // ative deepening for M − 1 Max moves, the search can be safely be
                // // stopped since it is not possible to find a better move. A deeper search
                // // will always return a worse probability than the previous search be-
                // // cause of strategy fusion. Therefore if the probability is equal to the
                // // one of the best move of the previous shallower search the probability
                // // cannot be improved and a better move cannot be found so it is safe
                // // to cut.
                // if s.is_empty()
                //     && self.use_optimizations
                //     && table_score.is_some()
                //     && front.score() == table_score.unwrap()
                // {
                //     trace!("root cut optiminzation for: {:?} on action: {}", s, a);
                //     break;
                // }
            }
        }

        assert!(!front.is_empty());

        if self.use_optimizations {
            let cache_value = TableValue {
                front: front.clone(),
                action: best_action,
                is_max_node,
            };
            self.cache.insert(s, cache_value);
        }
        trace!(
            "found front for alpha mu: {:?}, score: {}, {:?}",
            s,
            front.score(),
            front
        );
        worlds.clear();
        self.cache.world_vector_pool.attach(worlds);

        (front, best_action)
    }

    /// Returns a sorted list of moves to ensure deterministic move selection. If m > 1, it will return
    /// the previous best move in the first position if it is still viable.
    ///
    /// It's critical that moves are always returned in the same order or the recommended
    /// move will change if there are equally scored moves
    fn all_moves(&self, worlds: &[WorldState<G>], t: &Option<&TableValue>) -> Vec<Action> {
        let mut all_moves = HashSet::new();
        let mut actions = Vec::new();

        // only take actions from useful worlds
        for w in worlds.iter().filter(|x| x.is_useful()).flatten() {
            w.legal_actions(&mut actions);
            for a in &actions {
                all_moves.insert(*a);
            }
        }
        let mut sorted_moves = all_moves.into_iter().collect_vec();
        sorted_moves.sort();

        // Add the best move from previous, shallower searches to the front. This
        // won't happen in m=1 scenarios, so it doesn't impact matching the PIMCTS results
        // there may be no possible moves because we've filtered out all remaining worlds
        // as useless
        if !sorted_moves.is_empty() {
            if let Some(t) = t {
                if let Some(a) = t.action {
                    // This move may not be possible any more due to useless worlds
                    let guess_move = sorted_moves.iter().position(|&x| x == a).unwrap_or(0);
                    sorted_moves.swap(0, guess_move);
                }
            }
        }

        sorted_moves
    }

    /// Returns the progressed worlds where `a` was a valid action. Otherwise it
    /// marks that spot as None
    fn filter_and_progress_worlds(
        &mut self,
        worlds: &Vec<WorldState<G>>,
        a: Action,
    ) -> Vec<WorldState<G>> {
        let mut worlds_1 = self.cache.world_vector_pool.detach();
        worlds_1.clear();
        let mut actions = Vec::new();
        for w in worlds.iter() {
            if w.is_invalid() {
                worlds_1.push(WorldState::Invalid);
                continue;
            } else if w.is_useless() {
                worlds_1.push(WorldState::Useless);
                continue;
            }
            let w = w.unwrap();
            w.legal_actions(&mut actions);
            if actions.contains(&a) {
                let nw = self.play(w, a);
                worlds_1.push(WorldState::Useful(nw));
            } else {
                worlds_1.push(WorldState::Invalid)
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

    fn stop(&mut self, m: usize, worlds: &[WorldState<G>], result: &mut AMFront) -> bool {
        // Add the valid world cut from the optimizing alpha mu paper:
        //
        // Search can also be cut if there is only one useful world
        // left. The reason for this is that all of the useless worlds will
        // eventually evaluate to zero at the root, and so we only need
        // to compute the DDS result associated to the single useful
        // world and we can return a single vector containing the result
        // for the useful world.
        let useful_worlds = worlds.iter().filter(|x| x.is_useful()).count();
        if m == 0 || useful_worlds <= 1 {
            for (i, w) in worlds.iter().enumerate().filter(|(_, w)| !w.is_invalid()) {
                if w.is_useful() {
                    let w = w.unwrap();
                    let v = self.evaluator.evaluate_player(w, self.team.into());
                    assert!(v != 0.0);
                    assert!(v as i8 != 0);
                    result.set(i, VectorValue::from(v as i8));
                } else if w.is_useless() {
                    result.set(i, USELESS_WORLD_VALUE);
                }
            }
            return true;
        }

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
            for (i, w) in worlds.iter().enumerate().filter(|(_, w)| !w.is_invalid()) {
                if w.is_useful() {
                    let w = w.unwrap();
                    let v = w.evaluate(self.team.into());
                    result.set(i, VectorValue::from(v as i8));
                } else if w.is_useless() {
                    result.set(i, USELESS_WORLD_VALUE);
                }
            }
            return true;
        }

        false
    }

    fn reset(&mut self) {
        self.cache.reset();
    }

    fn get_team(&self, worlds: &[WorldState<G>]) -> Team {
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

    /// Finds and sets useless worlds
    ///
    /// If at a Min node the maximum value of a world for the
    /// current Pareto front is zero, the world can be marked as use-
    /// less as it will always have a zero value in the Pareto front
    /// returned by the node.
    fn update_useful_worlds(&self, front: &AMFront, worlds: &mut [WorldState<G>]) {
        if !self.use_optimizations {
            return;
        }

        for (i, w) in worlds.iter_mut().enumerate().filter(|(_, w)| w.is_useful()) {
            let max = front.world_max(i);
            if max.is_some() && max.unwrap() <= USELESS_WORLD_VALUE {
                *w = WorldState::Useless;
            }
        }
    }

    /// Determine if a deep alpha cut is possible
    ///
    /// From alpha mu optimization paper
    fn is_dominated_by_upper_max(&self, s: &[Action], front: &AMFront) -> bool {
        if !self.use_optimizations {
            return false;
        }

        let mut node = s.to_owned();

        loop {
            node.pop();
            let value = self.cache.get(&node);

            if value.is_none() {
                return false;
            } else if !value.unwrap().is_max_node {
                continue;
            } else if let Some(v) = value {
                if front.less_than_or_equal(&v.front) {
                    return true;
                }
            }

            // didn't find a dominating value before the root node
            if node.is_empty() {
                return false;
            }
        }
    }
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Policy<G> for AlphaMuBot<G, E> {
    fn action_probabilities(&mut self, gs: &G) -> ActionVec<f64> {
        let actions = actions!(gs);
        let mut policy = ActionVec::new(&actions);

        let (_, action) = self.run_search(gs, gs.cur_player());

        policy[action.unwrap()] = 1.0;
        policy
    }
}

impl<G: GameState + ResampleFromInfoState, E: Evaluator<G>> Evaluator<G> for AlphaMuBot<G, E> {
    fn evaluate(&mut self, _gs: &G) -> Vec<f64> {
        todo!()
    }

    fn evaluate_player(&mut self, gs: &G, p: Player) -> f64 {
        let (score, _) = self.run_search(gs, p);
        score
    }

    fn prior(&mut self, _gs: &G) -> ActionVec<f64> {
        todo!()
    }
}

struct AlphaMuCache<G> {
    world_vector_pool: Pool<Vec<WorldState<G>>>,
    transposition_table: HashMap<u64, TableValue>,
}

struct TableValue {
    front: AMFront,
    action: Option<Action>,
    is_max_node: bool,
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
    fn get(&self, s: &[Action]) -> Option<&TableValue> {
        let mut hasher = DefaultHasher::default();
        s.hash(&mut hasher);
        self.transposition_table.get(&hasher.finish())
    }

    pub fn insert(&mut self, s: &[Action], v: TableValue) {
        // shouldn't be storing empty fronts
        assert!(!v.front.is_empty());
        let mut hasher = DefaultHasher::default();
        s.hash(&mut hasher);
        self.transposition_table.insert(hasher.finish(), v);
    }

    pub fn reset(&mut self) {
        self.transposition_table.clear();
    }
}

/// Helper function for printing worlds
fn get_worlds_string<G: Display>(worlds: &[WorldState<G>]) -> String {
    let mut s = String::new();
    for w in worlds.iter() {
        s.push_str(format!("\n\t{} ", w).as_str())
    }
    s
}

#[cfg(test)]
mod tests {

    use rand::{seq::SliceRandom, thread_rng, SeedableRng};

    use crate::{
        actions,
        agents::{Agent, PolicyAgent, RandomAgent},
        algorithms::ismcts::RandomRolloutEvaluator,
        game::{kuhn_poker::KuhnPoker, GameState},
    };

    use super::AlphaMuBot;

    #[test]
    fn test_alpha_mu_is_agent() {
        let mut alphamu = PolicyAgent::new(
            AlphaMuBot::new(
                RandomRolloutEvaluator::new(1),
                20,
                10,
                SeedableRng::seed_from_u64(42),
            ),
            SeedableRng::seed_from_u64(43),
        );
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

    // #[test]
    // fn alpha_mu_consistency() {
    //     let gs = EuchreGameState::from(
    //         "KcTsJsQsAd|9cTcAcKsAs|ThKh9dJdKd|JcJhQhAhQd|Qc|PT|Ah|Ad9c9dQd|AsJd",
    //     );

    //     let mut alphamu =
    //         AlphaMuBot::new(OpenHandSolver::new(), 3, 3, SeedableRng::seed_from_u64(42));
    //     let policy = alphamu.evaluate_player(&gs, gs.cur_player());

    //     for _ in 0..1 {
    //         let mut alphamu =
    //             AlphaMuBot::new(OpenHandSolver::new(), 3, 3, SeedableRng::seed_from_u64(42));
    //         alphamu.use_optimizations = false;
    //         assert_eq!(alphamu.evaluate_player(&gs, gs.cur_player()), policy);
    //     }
    // }
}
