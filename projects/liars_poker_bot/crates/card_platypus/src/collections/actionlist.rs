use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

use games::istate::NormalizedAction;

/// Backing type for [`ActionList`]'s bitmask. Each Action discriminant
/// occupies one bit; the choice of integer width controls how many distinct
/// IDs the list can store and — equally important — what the on-disk
/// [`InfoState`](crate::algorithms::cfres::InfoState) bucket layout looks
/// like when the store is memory-mapped via `bytemuck::cast_slice`.
///
/// We expose this as a trait so each game can pick the *smallest* width
/// that fits its action space:
///
///   * Euchre: 32 distinct Action discriminants → `u32`. Saves 4 bytes per
///     stored slot (≈5 GB on the 91 GB `three_card_played` mmap) and keeps
///     the bucket layout binary-compatible with the Euchre weight files
///     that were trained before the November 2026 `u32` → `u64` change.
///   * Oh Hell: 52-card deck + bid actions occupy IDs 0..=63 → `u64`.
///     `u32` here silently wraps high IDs modulo 32 and corrupts training.
///
/// Implemented only for `u32` and `u64`. Adding a new width needs explicit
/// review of every persisted mmap that uses it.
pub trait ActionMask:
    Pod + Zeroable + Default + Copy + Send + Sync + Serialize + for<'de> Deserialize<'de>
{
    /// Highest Action ID this width can represent (`8 * size_of::<Self>() - 1`).
    const MAX_ID: u8;

    fn count_ones(self) -> u32;
    fn trailing_zeros(self) -> u32;
    fn has_bit(self, bit: u8) -> bool;
    fn with_bit_set(self, bit: u8) -> Self;
    fn with_bit_cleared(self, bit: u8) -> Self;
    /// Number of set bits strictly below position `bit`. Used to look up
    /// the dense index of a particular Action within the bitmask.
    fn count_below(self, bit: u8) -> u32;
}

impl ActionMask for u32 {
    const MAX_ID: u8 = 31;
    fn count_ones(self) -> u32 {
        u32::count_ones(self)
    }
    fn trailing_zeros(self) -> u32 {
        u32::trailing_zeros(self)
    }
    fn has_bit(self, bit: u8) -> bool {
        self & (1u32 << bit) != 0
    }
    fn with_bit_set(self, bit: u8) -> Self {
        self | (1u32 << bit)
    }
    fn with_bit_cleared(self, bit: u8) -> Self {
        self & !(1u32 << bit)
    }
    fn count_below(self, bit: u8) -> u32 {
        let mask = !(!0u32 << bit);
        (self & mask).count_ones()
    }
}

impl ActionMask for u64 {
    const MAX_ID: u8 = 63;
    fn count_ones(self) -> u32 {
        u64::count_ones(self)
    }
    fn trailing_zeros(self) -> u32 {
        u64::trailing_zeros(self)
    }
    fn has_bit(self, bit: u8) -> bool {
        self & (1u64 << bit) != 0
    }
    fn with_bit_set(self, bit: u8) -> Self {
        self | (1u64 << bit)
    }
    fn with_bit_cleared(self, bit: u8) -> Self {
        self & !(1u64 << bit)
    }
    fn count_below(self, bit: u8) -> u32 {
        let mask = !(!0u64 << bit);
        (self & mask).count_ones()
    }
}

/// Compact representation of the legal-action set at an info state. Each
/// possible Action discriminant (u8) is encoded as a single bit on top of
/// a backing `T: ActionMask`.
///
/// Generic over the backing type so Euchre can keep its 4-byte
/// representation (and reuse pre-`u64`-default weight files) while Oh Hell
/// gets the 8-byte representation it needs for IDs ≥ 32.
///
/// The default `T = u64` matches the safer historical default — using `u32`
/// for a game with >32 IDs silently truncates bits and corrupts policies,
/// which is the failure mode the original `u32` → `u64` bump was fixing.
// `serde`'s derive macros add `T: Serialize` / `T: Deserialize` bounds
// automatically. We instead bound on `T: ActionMask`, which is a supertrait
// of both — this keeps the derive's generated impls in sync with our
// `ActionMask` bound everywhere else and avoids the macro inferring an
// orphan `T: Deserialize<'_>` bound that the rest of the code doesn't
// satisfy generically.
#[derive(Serialize, Deserialize, Default, Clone, Copy)]
#[serde(bound(serialize = "T: ActionMask", deserialize = "T: ActionMask"))]
#[repr(transparent)]
pub struct ActionList<T: ActionMask = u64>(T);

// SAFETY: `repr(transparent)` over a Pod backing means the wrapper has the
// same byte layout as `T`. All zero bytes → `T::default()` → empty list.
unsafe impl<T: ActionMask> Pod for ActionList<T> {}
unsafe impl<T: ActionMask> Zeroable for ActionList<T> {}

impl<T: ActionMask> ActionList<T> {
    pub fn new(actions: &[NormalizedAction]) -> Self {
        let mut list = Self::default();
        for a in actions {
            list.insert(*a);
        }
        list
    }

    /// Returns the dense index of `a` within the bitmask (i.e. the position
    /// of its bit when iterating set bits low-to-high). `None` if `a` is
    /// not present. Used to pair each present Action with its corresponding
    /// weight in the parallel `regrets` / `avg_strategy` arrays.
    pub fn index(&self, a: NormalizedAction) -> Option<usize> {
        if !self.contains(a) {
            return None;
        }
        let id = a.get().0;
        Some(self.0.count_below(id) as usize)
    }

    pub fn contains(&self, a: NormalizedAction) -> bool {
        self.0.has_bit(a.get().0)
    }

    pub fn insert(&mut self, a: NormalizedAction) {
        let id = a.get().0;
        debug_assert!(
            id <= T::MAX_ID,
            "ActionList<{}> only supports Action IDs in 0..={}, got {}",
            std::any::type_name::<T>(),
            T::MAX_ID,
            id,
        );
        self.0 = self.0.with_bit_set(id);
    }

    pub fn len(&self) -> usize {
        self.0.count_ones() as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_vec(&self) -> Vec<NormalizedAction> {
        let mut mask = self.0;
        let mut actions = Vec::with_capacity(self.len());

        while mask.count_ones() > 0 {
            let id = mask.trailing_zeros() as u8;
            actions.push(NormalizedAction::new_from_id(id));
            mask = mask.with_bit_cleared(id);
        }

        actions
    }
}

#[cfg(test)]
mod tests {
    use games::istate::NormalizedAction;

    use super::ActionList;

    #[test]
    fn test_action_list() {
        let mut list: ActionList<u64> = ActionList::default();

        assert_eq!(list.len(), 0);

        list.insert(NormalizedAction::new_from_id(1));

        assert!(list.contains(NormalizedAction::new_from_id(1)));
        assert_eq!(list.len(), 1);
        assert_eq!(list.index(NormalizedAction::new_from_id(1)), Some(0));

        list.insert(NormalizedAction::new_from_id(0));
        assert!(list.contains(NormalizedAction::new_from_id(1)));
        assert_eq!(list.len(), 2);
        assert_eq!(list.index(NormalizedAction::new_from_id(1)), Some(1));
        assert_eq!(list.index(NormalizedAction::new_from_id(0)), Some(0));

        assert_eq!(
            list.to_vec(),
            vec![
                NormalizedAction::new_from_id(0),
                NormalizedAction::new_from_id(1)
            ]
        )
    }

    #[test]
    fn u32_and_u64_have_same_low_bits() {
        // A u32-backed list and a u64-backed list should agree on every
        // operation that involves only IDs in 0..32 — that's what makes
        // the Euchre `ActionList<u32>` a drop-in compact replacement.
        let mut a: ActionList<u32> = ActionList::default();
        let mut b: ActionList<u64> = ActionList::default();
        for id in [0u8, 5, 9, 14, 22, 31] {
            a.insert(NormalizedAction::new_from_id(id));
            b.insert(NormalizedAction::new_from_id(id));
        }
        assert_eq!(a.len(), b.len());
        for id in [0u8, 5, 9, 14, 22, 31] {
            let na = NormalizedAction::new_from_id(id);
            assert_eq!(a.contains(na), b.contains(na));
            assert_eq!(a.index(na), b.index(na));
        }
    }
}
