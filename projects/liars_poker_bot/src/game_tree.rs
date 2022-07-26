use std::cmp::max;

use crate::{
    game::GameState,
    liars_poker::{parse_bet, LPAction, LPGameState, Player},
};

/// Arena tree implementation
pub struct GameTree<G: Clone + GameState> {
    nodes: Vec<GameTreeNode<G>>,
}

impl<G: GameState + Clone> GameTree<G> {
    pub fn new(g: &G) -> GameTree<G> {
        let mut nodes_to_process = Vec::new();

        let mut tree = Self { nodes: Vec::new() };
        let actor = match g.get_acting_player() {
            Player::P1 => Player::P2,
            Player::P2 => Player::P1,
        };

        // Create the root node
        tree.nodes.push(GameTreeNode {
            children: Vec::new(),
            state: g.clone(),
            action: None,
            score: None,
            actor: actor,
        });

        nodes_to_process.push(0);

        while let Some(parent_id) = nodes_to_process.pop() {
            let parent = tree.get(parent_id);
            let state = parent.state.clone();
            let actions = state.get_actions();

            for a in actions {
                let mut next_state = state.clone();
                next_state.apply(&a);
                let child = tree.new_node(next_state, Some(a), Some(parent_id));
                nodes_to_process.push(child);
            }
        }
        tree.score_tree();

        return tree;
    }

    fn new_node(&mut self, state: G, action: Option<G::Action>, parent: Option<usize>) -> usize {
        // Get the next free index
        let next_index = self.nodes.len();

        let parent_actor = match parent {
            None => Player::P1,
            Some(id) => {
                let p = self.get(id);
                p.actor
            }
        };
        let actor = match parent_actor {
            Player::P1 => Player::P2,
            Player::P2 => Player::P1,
        };

        // Push the node into the arena
        self.nodes.push(GameTreeNode {
            children: Vec::new(),
            state: state,
            action: action,
            score: None,
            actor: actor,
        });

        if let Some(p) = parent {
            self.nodes[p].children.push(next_index);
        }

        // Return the node identifier
        return next_index;
    }

    pub fn get(&self, id: usize) -> &GameTreeNode<G> {
        return &self.nodes[id];
    }

    fn set_score(&mut self, id: usize, score: f32) {
        self.nodes[id].score = Some(score);
    }

    pub fn len(&self) -> usize {
        return self.nodes.len();
    }

    /// Use minimax algorithm to propogate scores up the tree
    fn score_tree(&mut self) {
        let mut nodes_to_score = Vec::new();
        nodes_to_score.push(0);

        'processor: while let Some(id) = nodes_to_score.pop() {
            let n = self.get(id);
            let mut score = match n.actor {
                Player::P1 => f32::MAX,
                Player::P2 => f32::MIN,
            };

            if n.children.len() == 0 {
                // leaf node
                let score = self.score_game_state(&n.state);
                self.set_score(id, score);
            } else {
                for &c in &n.children {
                    let cn = self.get(c);
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

                self.set_score(id, score);
            }
        }
    }

    /// Returns the chance of P1 winning from this game state
    fn score_game_state(&self, g: &G) -> f32 {
        let mut wins = 0;
        let mut games = 0;
        for s in g.get_possible_states() {
            wins += max(s.evaluate(), 0);
            games += 1;
        }

        return wins as f32 / games as f32;
    }
}

impl std::fmt::Debug for GameTree<LPGameState> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        const START: char = '├';
        const V: char = '|'; // vertical

        let mut output = String::new();
        let mut nodes_to_print = Vec::new();
        nodes_to_print.push((0, 0));
        while let Some((id, depth)) = nodes_to_print.pop() {
            for _ in 0..depth {
                output.push(V);
            }
            let node = self.get(id);
            let action_string = match node.action {
                Some(LPAction::Bet(x)) => {
                    let (n, v) = parse_bet(x);
                    format!("{} {}s", n, v)
                }
                Some(LPAction::Call) => "C".to_string(),
                _ => String::new(),
            };

            output.push_str(&format!("{} {:?} {:?}", START, node.actor, action_string));

            if let Some(score) = node.score {
                output.push_str(&format!(": {}", score));
            }

            output.push_str("\n");

            for c in &node.children {
                nodes_to_print.push((*c, depth + 1));
            }
        }

        write!(f, "{}", output)
    }
}

#[derive(Debug)]
pub struct GameTreeNode<G: Clone + GameState> {
    children: Vec<usize>,
    pub state: G,
    pub action: Option<G::Action>,
    pub actor: Player,
    pub score: Option<f32>,
}

#[cfg(test)]
mod tests {
    use crate::{
        game_tree::GameTree,
        liars_poker::{DiceState, LPGameState, Player, DICE_SIDES, NUM_DICE},
    };

    #[test]
    fn test_score_game_state() {
        let mut g = LPGameState {
            dice_state: [DiceState::K(0), DiceState::K(0), DiceState::U, DiceState::U],
            bet_state: [None; NUM_DICE * DICE_SIDES],
            call_state: None,
        };
        g.call_state = Some(Player::P2);

        g.bet_state[0] = Some(Player::P1);
        let t = GameTree::new(&g);
        let score = t.score_game_state(&g);
        assert_eq!(score, 1.0);

        g.bet_state[2 * DICE_SIDES] = Some(Player::P1);
        let score = t.score_game_state(&g);
        assert_eq!(
            score,
            2.0 / DICE_SIDES as f32 - 1.0 / DICE_SIDES as f32 / DICE_SIDES as f32
        );

        g.bet_state[2 * DICE_SIDES + 1] = Some(Player::P1);
        let score = t.score_game_state(&g);
        assert_eq!(score, 0.0);
    }
}
