use rand::Rng;

use crate::Player;

/// Resample the chance nodes to other versions of the gamestate that result in the same istate for a given player
pub trait ResampleFromInfoState {
    fn resample_from_istate<T: Rng>(&self, player: Player, rng: &mut T) -> Self;
}
