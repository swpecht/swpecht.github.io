use std::fmt::Display;

use crate::{
    gamestate::{SimCoords, SimState, Team, UnitType},
    ModelSprite,
};

pub fn insert_space_marine_unit(gs: &mut SimState, locs: Vec<SimCoords>, team: Team) {
    for (i, l) in locs.into_iter().enumerate() {
        let unit_type = if i == 0 {
            UnitType::NewUnit
        } else {
            UnitType::LastUnit
        };

        // https://wahapedia.ru/wh40k10ed/factions/space-marines/datasheets.html#Tactical-Squad
        gs.insert_model(
            ModelSprite::Knight,
            l,
            team,
            unit_type,
            ModelStats {
                movement: 6,
                wound: 2,
                toughness: 4,
                save: 3,
            },
            vec![
                Weapon::BoltPistol,
                Weapon::Boltgun,
                Weapon::CloseCombatWeapon,
            ],
        );
    }
}

pub fn insert_necron_unit(gs: &mut SimState, locs: Vec<SimCoords>, team: Team) {
    for (i, l) in locs.into_iter().enumerate() {
        let unit_type = if i == 0 {
            UnitType::NewUnit
        } else {
            UnitType::LastUnit
        };

        // https://wahapedia.ru/wh40k10ed/factions/necrons/Necron-Warriors
        gs.insert_model(
            ModelSprite::Skeleton,
            l,
            team,
            unit_type,
            ModelStats {
                movement: 5,
                wound: 1,
                toughness: 4,
                save: 4,
            },
            vec![Weapon::GaussFlayer],
        );
    }
}

// https://wahapedia.ru/wh40k10ed/factions/space-marines/datasheets.html#Tactical-Squad

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct WeaponStats {
    pub range: u8,
    pub num_attacks: RollableValue,
    pub skill: u8,
    pub strength: u8,
    pub armor_penetration: u8,
    pub damage: u8,
}

#[derive(Hash, Debug, PartialEq, Clone)]
pub struct ModelStats {
    pub movement: u8,
    pub wound: u8,
    pub toughness: u8,
    pub save: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RollableValue {
    #[default]
    One,
    Two,
    Three,
    D6,
    D3,
}

impl RollableValue {
    pub fn value(&self) -> u8 {
        match self {
            RollableValue::One => 1,
            RollableValue::Two => 2,
            RollableValue::Three => 3,
            RollableValue::D6 => todo!(),
            RollableValue::D3 => todo!(),
        }
    }
}

// https://wahapedia.ru/wh40k10ed/factions/space-marines/datasheets.html#Tactical-Squad
#[derive(PartialEq, Debug, Default, Clone, Hash, Eq, Copy, Ord, PartialOrd)]
pub enum Weapon {
    #[default]
    BoltPistol,
    Boltgun,
    Flamer,
    MissleLauncherFrag,
    GaussFlayer,
    CloseCombatWeapon,
}

impl Weapon {
    pub fn stats(&self) -> WeaponStats {
        match self {
            Weapon::BoltPistol => WeaponStats {
                range: 12,
                num_attacks: RollableValue::One,
                skill: 3,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            Weapon::Boltgun => WeaponStats {
                range: 24,
                num_attacks: RollableValue::Two,
                skill: 3,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            Weapon::Flamer => WeaponStats {
                range: 12,
                num_attacks: RollableValue::D6,
                skill: 0, // torrent weapon so always hits
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            Weapon::MissleLauncherFrag => WeaponStats {
                range: 48,
                num_attacks: RollableValue::D6,
                skill: 4,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            Weapon::GaussFlayer => WeaponStats {
                range: 24,
                num_attacks: RollableValue::One,
                skill: 4,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            Weapon::CloseCombatWeapon => WeaponStats {
                range: 0,
                num_attacks: RollableValue::Two,
                skill: 3,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
        }
    }
}

impl Display for Weapon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Weapon::*;
        f.write_str(match self {
            BoltPistol => "Bolt pistol",
            Boltgun => "Boltgun",
            Flamer => "Flamer",
            MissleLauncherFrag => "Missle Launcher - Frag",
            GaussFlayer => "Gauss Flayer",
            CloseCombatWeapon => "Close Combat Weapon",
        })
    }
}
