use std::{cmp::Ordering, collections::HashSet, fmt::Debug, ops::Index};

use crate::game::{Action, GameState, Player};

/// Implementation for AlphaMu from "The αµ Search Algorithm for the Game of Bridge"
///
/// https://arxiv.org/pdf/1911.07960.pdf
pub struct AlphaMuBot<G> {
    worlds: Vec<G>,
    team: Team,
}

impl<G: GameState> AlphaMuBot<G> {
    pub fn run_search(&mut self, root_node: &G, m: usize, num_worlds: usize) {
        self.reset();

        self.team = self.get_team(root_node);

        self.alphamu(root_node, m, Vec::new());
    }

    fn alphamu(&mut self, gs: &G, m: usize, worlds: Vec<Option<G>>) -> AMFront {
        assert!(!gs.is_chance_node());

        let mut result = AMFront::default();
        if self.stop(gs, m, &worlds, &mut result) {
            return result;
        }

        let mut front = AMFront::default();
        if self.team != self.get_team(gs) {
            // min node
            for a in self.all_moves(&worlds) {
                let s = self.play(gs, a);
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let f = self.alphamu(&s, m, worlds_1);
                front = front.min(f);
            }
        } else {
            // max node
            for a in self.all_moves(&worlds) {
                let s = self.play(gs, a);
                let worlds_1 = self.filter_and_progress_worlds(&worlds, a);
                let f = self.alphamu(&s, m - 1, worlds_1);
                front = front.max(f);
            }
        }

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

    /// Returns the progressed worlds where a was a valid action. Otherwise it
    /// marks that spot as None
    fn filter_and_progress_worlds(&mut self, worlds: &Vec<Option<G>>, a: Action) -> Vec<Option<G>> {
        let mut worlds_1 = vec![None; worlds.len()];
        let mut actions = Vec::new();
        for w in worlds.iter().flatten() {
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

    fn stop(&mut self, gs: &G, m: usize, worlds: &[Option<G>], result: &mut AMFront) -> bool {
        if gs.is_terminal() {
            let value = gs.evaluate(self.team.into());
            if value > 0.0 {
                // we won
                for (i, _) in worlds.iter().enumerate() {
                    result.set(i, true);
                }
            } else {
                // we lost
                for (i, _) in worlds.iter().enumerate() {
                    result.set(i, false);
                }
            }

            return true;
        }

        if m == 0 {
            todo!()
        }

        false
    }

    fn reset(&mut self) {
        self.worlds.clear();
        todo!()
    }

    fn get_team(&self, gs: &G) -> Team {
        match gs.cur_player() {
            0 | 2 => Team::Team1,
            1 | 3 => Team::Team2,
            _ => panic!("invalid player"),
        }
    }
}

/// An alphamu vector
///
/// True means the game is won, false means it is lost
#[derive(PartialEq, Eq, Clone, Copy)]
struct AMVector {
    is_win: [bool; 32],
    is_valid: [bool; 32],
    len: usize,
}

impl AMVector {
    fn new(size: usize) -> Self {
        Self {
            is_win: [false; 32],
            is_valid: [false; 32],
            len: size,
        }
    }

    fn _from_slice(wins: &[bool]) -> Self {
        let mut v = Self::new(wins.len());
        for (i, w) in wins.iter().enumerate() {
            v.is_win[i] = *w;
            v.is_valid[i] = true;
        }
        v
    }

    fn min(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for (i, w) in self.is_win.iter_mut().enumerate() {
            *w = *w && other.is_win[i];
        }

        self
    }

    fn max(mut self, other: Self) -> Self {
        assert_eq!(self.len, other.len);

        for (i, w) in self.is_win.iter_mut().enumerate() {
            *w = *w || other.is_win[i];
        }

        self
    }
}

impl Debug for AMVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[").unwrap();

        for w in self.is_win.iter().take(self.len) {
            match w {
                true => write!(f, "1").unwrap(),
                false => write!(f, "0").unwrap(),
            }
        }

        write!(f, "]")
    }
}

impl PartialOrd for AMVector {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        assert_eq!(self.len, other.len);

        // A vector is greater or equal to another vector if for all indices it
        // contains a value greater or equal to the value contained at this index
        // in the other vector and if the valid worlds are the same for the two
        let same_valid_worlds = self.is_valid == other.is_valid;
        if !same_valid_worlds {
            return Some(std::cmp::Ordering::Less);
        }

        if self.is_win == other.is_win {
            return Some(std::cmp::Ordering::Equal);
        }

        let mut is_greater_or_equal = true;
        for (i, is_win) in self
            .is_win
            .iter()
            .enumerate()
            .filter(|(i, _)| self.is_valid[*i])
        {
            is_greater_or_equal &= *is_win >= other.is_win[i];
        }

        if is_greater_or_equal {
            return Some(Ordering::Greater);
        }

        Some(Ordering::Less)
    }
}

#[derive(Default)]
struct AMFront {
    front: Vec<AMVector>,
}

impl AMFront {
    fn min(self, other: Self) -> Self {
        todo!()
    }

    fn max(self, other: Self) -> Self {
        todo!()
    }

    fn set(&mut self, idx: usize, is_win: bool) {
        todo!()
    }
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Team {
    Team1,
    Team2,
}

impl From<Team> for Player {
    fn from(val: Team) -> Self {
        val as usize
    }
}

#[cfg(test)]
mod tests {
    use crate::algorithms::alphamu::AMVector;

    #[test]
    fn test_am_vector_ordering() {
        let mut v1 = AMVector::_from_slice(&[false, true, true]);
        let v2 = AMVector::_from_slice(&[false, false, true]);

        assert!(v1 == v1);
        assert!(v1 >= v2);
        assert!(v2 <= v1);

        v1.is_valid[2] = false;

        // Different valid worlds, each is less than the other
        assert!(v1 < v2);
        assert!(v2 < v1);
    }

    #[test]
    fn test_am_vector_min_max() {
        let v1 = AMVector::_from_slice(&[true, false, true]);
        let v2 = AMVector::_from_slice(&[true, true, false]);

        assert_eq!(v1.min(v2), AMVector::_from_slice(&[true, false, false]));
        assert_eq!(v1.max(v2), AMVector::_from_slice(&[true, true, true]));
    }

    #[test]
    fn test_am_front_min() {
        todo!()
    }

    #[test]
    fn test_am_front_max() {
        todo!()
    }
}
