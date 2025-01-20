macro_rules! unit_models {
    (  $x: expr, $unit: expr ) => {{
        $x.models
            .iter()
            .filter(move |m| m.unit == $unit && !m.is_destroyed)
    }};
}

macro_rules! unit_models_mut {
    (  $x: expr, $unit: expr ) => {{
        $x.models
            .iter_mut()
            .filter(move |m| m.unit == $unit && !m.is_destroyed)
    }};
}

macro_rules! team_models {
    (  $x: expr, $team: expr ) => {{
        $x.models
            .iter()
            .filter(move |m| m.team == $team && !m.is_destroyed)
    }};
}

macro_rules! team_models_mut {
    (  $x: expr, $team: expr ) => {{
        $x.models
            .iter_mut()
            .filter(move |m| m.team == $team && !m.is_destroyed)
    }};
}

pub(super) use team_models;
pub(super) use team_models_mut;
pub(super) use unit_models;
pub(super) use unit_models_mut;

use super::Team;

#[derive(PartialEq, Clone, Debug)]
pub(super) struct TeamFlags {
    flags: [bool; 2],
}

impl TeamFlags {
    pub fn new_false() -> Self {
        TeamFlags {
            flags: [false, false],
        }
    }

    fn to_index(team: Team) -> usize {
        match team {
            Team::Players => 0,
            Team::NPCs => 1,
        }
    }

    pub fn get(&self, team: Team) -> bool {
        self.flags[TeamFlags::to_index(team)]
    }

    pub fn set(&mut self, team: Team, value: bool) {
        self.flags[TeamFlags::to_index(team)] = value;
    }
}
