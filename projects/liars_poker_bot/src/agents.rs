use std::{fmt::Debug, marker::PhantomData};

use log::debug;
use rand::{prelude::SliceRandom, thread_rng, Rng};

use crate::{
    game::GameState,
    game_tree::{GameTree, GameTreeNode},
    liars_poker::{parse_highest_bet, DiceState, LPAction, LPGameState, Player},
};

/// Base class for agents based on gametrees
pub struct TreeAgent<G: GameState + Clone> {
    pub name: String,
    /// Contains all state for the agents
    tree: GameTree<G>,
    rollout: fn(&mut GameTree<G>),
    score: fn(&mut GameTreeNode<G>),
    propogate: fn(&mut GameTree<G>),
}

impl<G: GameState + Clone + PartialEq + Debug> Agent<G> for TreeAgent<G> {
    fn name(&self) -> &str {
        return &self.name;
    }

    fn play(&mut self, g: &G, _possible_children: &Vec<G>) -> G {
        // Pass of the rollout
        debug!("tree state before rollout: \n {:?}", self.tree);
        (self.rollout)(&mut self.tree);
        debug!("tree state after rollout: \n {:?}", self.tree);

        // Pass of the scorer to score all leaf nodes
        let mut i = 0;
        loop {
            let n = self.tree.get_mut(i);
            if let Some(n) = n {
                if n.state.is_terminal() {
                    (self.score)(n)
                }
            } else {
                break;
            }
            i += 1;
        }

        // Pass of the propogator
        (self.propogate)(&mut self.tree);

        // Find the target gamestate and find the move with the highest score
        let id = self.find_state(g).unwrap();
        let mut children = self.tree.get_children(id);

        // Shuffle the order of evaluating moves to choose a random one if multiple have
        // the same utility
        let mut rng = thread_rng();
        children.shuffle(&mut rng);
        let mut cur_max = f32::MIN;
        let mut cur_move = None;

        for c in children {
            let c = self.tree.get(c).unwrap();
            if c.score.unwrap() > cur_max {
                cur_max = c.score.unwrap();
                cur_move = Some(c.state.clone());
            }
        }
        return cur_move.unwrap();
    }
}

impl<G: GameState + Clone + PartialEq> TreeAgent<G> {
    pub fn new(
        name: &str,
        g: &G,
        rollout: fn(&mut GameTree<G>),
        score: fn(&mut GameTreeNode<G>),
        propogate: fn(&mut GameTree<G>),
    ) -> Self {
        return Self {
            name: name.to_string(),
            tree: GameTree::new(g),
            rollout,
            score,
            propogate,
        };
    }

    fn find_state(&self, g: &G) -> Option<usize> {
        let mut id = 0;
        loop {
            let n = self.tree.get(id);
            if let Some(n) = n {
                if n.state == *g {
                    return Some(id);
                }
            } else {
                break;
            }
            id += 1;
        }
        return None;
    }
}

pub fn minimax_propogation<G: GameState + Clone>(tree: &mut GameTree<G>) {
    let mut nodes_to_score = Vec::new();
    nodes_to_score.push(0);

    'processor: while let Some(id) = nodes_to_score.pop() {
        let n = tree.get(id).unwrap();
        let mut score = match n.actor {
            Player::P1 => f32::MAX,
            Player::P2 => f32::MIN,
        };

        if n.score.is_none() {
            for &c in &tree.get_children(id) {
                let cn = tree.get(c).unwrap();
                if let Some(cn_score) = cn.score {
                    score = match n.actor {
                        Player::P1 => score.min(cn_score),
                        Player::P2 => score.max(cn_score),
                    }
                } else {
                    nodes_to_score.push(id); // need to rescore this node
                    nodes_to_score.push(c);
                    continue 'processor;
                }
            }

            tree.set_score(id, score);
        }
    }
}

/// Scores nodes randomly
pub fn random_scorer<G: GameState + Clone>(n: &mut GameTreeNode<G>) {
    let mut rng = rand::thread_rng();
    n.score = Some(rng.gen());
}

pub fn full_rollout<G: GameState + Clone + PartialEq>(tree: &mut GameTree<G>) {
    let mut nodes_to_process = Vec::new();
    nodes_to_process.push(0);

    while let Some(parent_id) = nodes_to_process.pop() {
        let parent = tree.get(parent_id).unwrap();
        let state = parent.state.clone();
        let children = state.get_children();
        let cur_children = tree.get_children(parent_id);
        let mut cur_children_state = Vec::new();
        for c_id in cur_children {
            cur_children_state.push(tree.get(c_id).unwrap().state.clone())
        }

        for c in children {
            if cur_children_state.contains(&c) {
                continue;
            }
            let child = tree.new_node(c, Some(parent_id));
            nodes_to_process.push(child);
        }
    }
}

pub trait Agent<G>
where
    G: GameState,
{
    fn name(&self) -> &str;
    fn play(&mut self, g: &G, possible_children: &Vec<G>) -> G;
}

/// Agent that randomly chooses moves
pub struct RandomAgent<G: GameState> {
    state_type: PhantomData<G>,
}

impl<G: GameState + Clone> Agent<G> for RandomAgent<G> {
    fn name(&self) -> &str {
        return &"RandomAgent";
    }

    fn play(&mut self, _: &G, possible_moves: &Vec<G>) -> G {
        let mut rng = rand::thread_rng();
        return possible_moves.choose(&mut rng).unwrap().clone();
    }
}

impl<G: GameState> RandomAgent<G> {
    pub fn new(_: &G) -> Self {
        return Self {
            state_type: PhantomData,
        };
    }
}

/// Agent always plays the first action
pub struct AlwaysFirstAgent<G: GameState> {
    state_type: PhantomData<G>,
}

impl<G: GameState + Clone> Agent<G> for AlwaysFirstAgent<G> {
    fn name(&self) -> &str {
        return &"AlwaysFirstAgent";
    }

    fn play(&mut self, _: &G, possible_moves: &Vec<G>) -> G {
        return possible_moves[0].clone();
    }
}

impl<G: GameState> AlwaysFirstAgent<G> {
    pub fn new(_: &G) -> Self {
        return Self {
            state_type: PhantomData,
        };
    }
}

pub struct OwnDiceAgent {
    name: &'static str,
}

impl OwnDiceAgent {
    pub fn new(_: &LPGameState) -> Self {
        return Self {
            name: "OwnDiceAgent",
        };
    }
}

impl Agent<LPGameState> for OwnDiceAgent {
    fn name(&self) -> &str {
        return &self.name;
    }

    fn play(&mut self, g: &LPGameState, possible_moves: &Vec<LPGameState>) -> LPGameState {
        // count own dice
        let mut counts = [0; 6];
        for d in g.dice_state {
            match d {
                DiceState::K(x) => counts[x] += 1,
                _ => {}
            }
        }

        if let Some((count, value)) = parse_highest_bet(&g) {
            if count > counts[value] {
                let p = g.get_acting_player();
                let mut r = g.clone();
                r.apply(p, &LPAction::Call);
                return r;
            }
        }

        for a in possible_moves {
            if let Some((count, value)) = parse_highest_bet(a) {
                if counts[value] >= count {
                    return a.clone();
                }
            }
        }

        let p = g.get_acting_player();
        let mut r = g.clone();
        r.apply(p, &LPAction::Call);
        return r;
    }
}
