use crate::game::GameState;

/// Implementation of ouctome sampling with lazy-weighted averaging and
/// epsilon-on-policy exploration
///
/// This is adapted from pg 50: http://mlanctot.info/files/papers/PhD_Thesis_MarcLanctot.pdf
pub fn outcome_sampling(g: &dyn GameState, p0: f32, p1: f32) -> f32 {
    if g.is_terminal() {
        return g.evaluate()[g.cur_player()] / (p0 * p1);
    }

    assert!(!g.is_chance_node());

    

    todo!();
}
