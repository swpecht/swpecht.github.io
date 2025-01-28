use itertools::{Itertools, Product};

use crate::gamestate::utils::team_models;

use super::{SimState, Team, WORLD_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SimCoords {
    pub x: usize,
    pub y: usize,
}

impl core::ops::Add for SimCoords {
    type Output = SimCoords;

    fn add(self, rhs: Self) -> Self::Output {
        let mut out = self;
        out.x += rhs.x;
        out.y += rhs.y;
        out
    }
}

impl core::ops::Sub for SimCoords {
    type Output = SimCoords;

    fn sub(self, rhs: Self) -> Self::Output {
        let mut out = self;
        out.x -= rhs.x;
        out.y -= rhs.y;
        out
    }
}

impl SimCoords {
    pub fn dist(&self, other: &SimCoords) -> usize {
        self.x.abs_diff(other.x) + self.y.abs_diff(other.y)
    }
}

pub fn sc(x: usize, y: usize) -> SimCoords {
    SimCoords { x, y }
}

/// Iterator over all world coords within distance d
pub(super) struct CoordIterator {
    max_range: usize,
    min_range: usize,
    middle: SimCoords,
    raw_iterator: Product<std::ops::Range<usize>, std::ops::Range<usize>>,
}

impl CoordIterator {
    pub fn new(middle: SimCoords, max_range: u8, min_range: u8) -> Self {
        let min_x = middle.x.saturating_sub(max_range as usize);
        let min_y = middle.y.saturating_sub(max_range as usize);
        let max_x = (middle.x + max_range as usize).min(WORLD_SIZE);
        let max_y = (middle.y + max_range as usize).min(WORLD_SIZE);

        let raw_iterator = (min_x..max_x + 1).cartesian_product(min_y..max_y + 1);

        Self {
            max_range: max_range as usize,
            middle,
            raw_iterator,
            min_range: min_range as usize,
        }
    }
}

impl Iterator for CoordIterator {
    type Item = SimCoords;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let cp = self.raw_iterator.next()?;
            let coord = sc(cp.0, cp.1);
            let dist = coord.dist(&self.middle);
            if dist <= self.max_range && dist >= self.min_range {
                return Some(coord);
            }
        }
    }
}

impl SimState {
    /// Returns if a given location is within engagement range of provided team
    pub(super) fn is_engagement_range(&self, loc: &SimCoords, team: Team) -> bool {
        const ENGAGEMENT_DISTANCE: usize = 1;
        team_models!(self, team)
            .any(|m| self.get_loc(m.id).unwrap().dist(loc) <= ENGAGEMENT_DISTANCE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        gamestate::{SimState, Team},
        info::{insert_necron_warrior_unit, insert_space_marine_unit},
    };

    #[test]
    fn test_engagement_range_check() {
        let mut gs = SimState::new();
        insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
        insert_necron_warrior_unit(&mut gs, vec![sc(2, 10)], Team::NPCs);

        assert!(!gs.is_engagement_range(&sc(1, 11), Team::NPCs));
        assert!(gs.is_engagement_range(&sc(1, 11), Team::Players));

        assert!(!gs.is_engagement_range(&sc(5, 5), Team::Players));
        assert!(!gs.is_engagement_range(&sc(5, 5), Team::NPCs));
    }
}
