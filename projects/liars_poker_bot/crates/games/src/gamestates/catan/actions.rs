//! Action encoding for Catan.
//!
//! `Action` is a u8, so the whole action space must fit in 256 ids. Player
//! decisions get dedicated, non-overlapping ranges. Chance outcomes that
//! select a card from a multiset (dev-card draws, robber steals) reuse low
//! ids 0..n as "slot" indices into the sorted multiset; this keeps the space
//! small and makes uniform sampling over legal actions produce correctly
//! weighted outcomes. Slot actions only occur in chance phases, so they never
//! collide with player actions at the same node, but decoding an action id
//! requires knowing the phase it was applied in.

use crate::Action;

use super::board::{Resource, RESOURCES};

pub const END_TURN: u8 = 0;
pub const BUY_DEV: u8 = 1;
pub const PLAY_KNIGHT: u8 = 2;
pub const PLAY_ROAD_BUILDING: u8 = 3;
pub const PLAY_YEAR_OF_PLENTY: u8 = 4;
pub const PLAY_MONOPOLY: u8 = 5;
/// 5 ids: pick a resource (discarding, year of plenty, monopoly).
pub const PICK_RESOURCE_BASE: u8 = 6;
/// 20 ids: trade with the bank/port, give resource r get resource g.
pub const TRADE_BASE: u8 = 11;
/// 19 ids: move the robber to a hex.
pub const MOVE_ROBBER_BASE: u8 = 31;
/// 4 ids: choose which player to steal from.
pub const STEAL_FROM_BASE: u8 = 50;
/// 6 ids: chance outcome of one die, value 1-6.
pub const DIE_ROLL_BASE: u8 = 54;
/// 54 ids: build a settlement on a vertex.
pub const SETTLEMENT_BASE: u8 = 60;
/// 54 ids: upgrade a settlement to a city.
pub const CITY_BASE: u8 = 114;
/// 72 ids: build a road on an edge.
pub const ROAD_BASE: u8 = 168;
pub const MAX_ACTION: u8 = 240;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatanAction {
    EndTurn,
    BuyDev,
    PlayKnight,
    PlayRoadBuilding,
    PlayYearOfPlenty,
    PlayMonopoly,
    PickResource(Resource),
    /// Trade `give` for `get` with the bank at the player's best rate.
    BankTrade { give: Resource, get: Resource },
    MoveRobber(u8),
    StealFrom(usize),
    DieRoll(u8),
    BuildSettlement(u8),
    BuildCity(u8),
    BuildRoad(u8),
    /// Chance: index into a sorted multiset (dev deck or victim's hand).
    ChanceSlot(u8),
}

impl CatanAction {
    pub fn to_action(self) -> Action {
        let id = match self {
            CatanAction::EndTurn => END_TURN,
            CatanAction::BuyDev => BUY_DEV,
            CatanAction::PlayKnight => PLAY_KNIGHT,
            CatanAction::PlayRoadBuilding => PLAY_ROAD_BUILDING,
            CatanAction::PlayYearOfPlenty => PLAY_YEAR_OF_PLENTY,
            CatanAction::PlayMonopoly => PLAY_MONOPOLY,
            CatanAction::PickResource(r) => PICK_RESOURCE_BASE + r as u8,
            CatanAction::BankTrade { give, get } => {
                let g = give as u8;
                let mut t = get as u8;
                assert_ne!(give, get);
                if t > g {
                    t -= 1;
                }
                TRADE_BASE + g * 4 + t
            }
            CatanAction::MoveRobber(h) => MOVE_ROBBER_BASE + h,
            CatanAction::StealFrom(p) => STEAL_FROM_BASE + p as u8,
            CatanAction::DieRoll(v) => {
                assert!((1..=6).contains(&v));
                DIE_ROLL_BASE + v - 1
            }
            CatanAction::BuildSettlement(v) => SETTLEMENT_BASE + v,
            CatanAction::BuildCity(v) => CITY_BASE + v,
            CatanAction::BuildRoad(e) => ROAD_BASE + e,
            CatanAction::ChanceSlot(s) => s,
        };
        Action(id)
    }

    /// Decode a player/dice action id. Must not be used for chance-slot
    /// phases (dev draw, steal card); those reuse low ids and are
    /// context-dependent.
    pub fn from_action(a: Action) -> CatanAction {
        let id = a.0;
        match id {
            END_TURN => CatanAction::EndTurn,
            BUY_DEV => CatanAction::BuyDev,
            PLAY_KNIGHT => CatanAction::PlayKnight,
            PLAY_ROAD_BUILDING => CatanAction::PlayRoadBuilding,
            PLAY_YEAR_OF_PLENTY => CatanAction::PlayYearOfPlenty,
            PLAY_MONOPOLY => CatanAction::PlayMonopoly,
            _ if (PICK_RESOURCE_BASE..TRADE_BASE).contains(&id) => {
                CatanAction::PickResource(RESOURCES[(id - PICK_RESOURCE_BASE) as usize])
            }
            _ if (TRADE_BASE..MOVE_ROBBER_BASE).contains(&id) => {
                let x = id - TRADE_BASE;
                let give = RESOURCES[(x / 4) as usize];
                let mut t = x % 4;
                if t >= give as u8 {
                    t += 1;
                }
                CatanAction::BankTrade {
                    give,
                    get: RESOURCES[t as usize],
                }
            }
            _ if (MOVE_ROBBER_BASE..STEAL_FROM_BASE).contains(&id) => {
                CatanAction::MoveRobber(id - MOVE_ROBBER_BASE)
            }
            _ if (STEAL_FROM_BASE..DIE_ROLL_BASE).contains(&id) => {
                CatanAction::StealFrom((id - STEAL_FROM_BASE) as usize)
            }
            _ if (DIE_ROLL_BASE..SETTLEMENT_BASE).contains(&id) => {
                CatanAction::DieRoll(id - DIE_ROLL_BASE + 1)
            }
            _ if (SETTLEMENT_BASE..CITY_BASE).contains(&id) => {
                CatanAction::BuildSettlement(id - SETTLEMENT_BASE)
            }
            _ if (CITY_BASE..ROAD_BASE).contains(&id) => CatanAction::BuildCity(id - CITY_BASE),
            _ if (ROAD_BASE..MAX_ACTION).contains(&id) => CatanAction::BuildRoad(id - ROAD_BASE),
            _ => panic!("invalid catan action id: {id}"),
        }
    }
}

impl From<CatanAction> for Action {
    fn from(value: CatanAction) -> Self {
        value.to_action()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_encoding_round_trips() {
        let mut all = vec![
            CatanAction::EndTurn,
            CatanAction::BuyDev,
            CatanAction::PlayKnight,
            CatanAction::PlayRoadBuilding,
            CatanAction::PlayYearOfPlenty,
            CatanAction::PlayMonopoly,
        ];
        for r in RESOURCES {
            all.push(CatanAction::PickResource(r));
            for g in RESOURCES {
                if r != g {
                    all.push(CatanAction::BankTrade { give: r, get: g });
                }
            }
        }
        for h in 0..19 {
            all.push(CatanAction::MoveRobber(h));
        }
        for p in 0..4 {
            all.push(CatanAction::StealFrom(p));
        }
        for v in 1..=6 {
            all.push(CatanAction::DieRoll(v));
        }
        for v in 0..54 {
            all.push(CatanAction::BuildSettlement(v));
            all.push(CatanAction::BuildCity(v));
        }
        for e in 0..72 {
            all.push(CatanAction::BuildRoad(e));
        }

        let mut seen = std::collections::HashSet::new();
        for a in all {
            let encoded = a.to_action();
            assert!(encoded.0 < MAX_ACTION);
            assert!(seen.insert(encoded.0), "duplicate id for {a:?}");
            assert_eq!(CatanAction::from_action(encoded), a);
        }
    }
}
