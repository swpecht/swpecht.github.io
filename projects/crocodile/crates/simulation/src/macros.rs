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
