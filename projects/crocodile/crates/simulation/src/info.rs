use std::fmt::Display;

use crate::{
    gamestate::{sc, SimCoords, SimState, Team},
    ModelSprite,
};

pub fn insert_space_marine_unit(
    gs: &mut SimState,
    loc: SimCoords,
    team: Team,
    unit: u8,
    num_models: usize,
) {
    for i in 0..num_models {
        gs.insert_model(
            ModelSprite::Knight,
            sc(loc.x + i, loc.y),
            team,
            unit,
            ModelStats {
                movement: 6,
                wound: 2,
            },
            vec![RangedWeapon::BoltPistol, RangedWeapon::Boltgun],
        );
    }
}

pub fn insert_necron_unit(
    gs: &mut SimState,
    loc: SimCoords,
    team: Team,
    unit: u8,
    num_models: usize,
) {
    for i in 0..num_models {
        // https://wahapedia.ru/wh40k10ed/factions/necrons/Necron-Warriors
        gs.insert_model(
            ModelSprite::Skeleton,
            sc(loc.x + i, loc.y),
            team,
            unit,
            ModelStats {
                movement: 5,
                wound: 1,
            },
            vec![RangedWeapon::GaussFlayer],
        );
    }
}

// https://wahapedia.ru/wh40k10ed/factions/space-marines/datasheets.html#Tactical-Squad

pub struct RangedWeaponStats {
    pub range: u8,
    pub attack: AttackValue,
    pub ballistic_skill: u8,
    pub strength: u8,
    pub armor_penetration: u8,
    pub damage: u8,
}

#[derive(Hash, Debug, PartialEq, Clone)]
pub struct ModelStats {
    pub movement: u8,
    pub wound: u8,
}

pub enum AttackValue {
    One,
    Two,
    Three,
    D6,
    D3,
}

// https://wahapedia.ru/wh40k10ed/factions/space-marines/datasheets.html#Tactical-Squad
#[derive(PartialEq, Debug, Default, Clone, Hash)]
pub enum RangedWeapon {
    #[default]
    BoltPistol,
    Boltgun,
    Flamer,
    MissleLauncherFrag,
    GaussFlayer,
}

impl RangedWeapon {
    pub fn stats(&self) -> RangedWeaponStats {
        match self {
            RangedWeapon::BoltPistol => RangedWeaponStats {
                range: 12,
                attack: AttackValue::One,
                ballistic_skill: 3,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            RangedWeapon::Boltgun => RangedWeaponStats {
                range: 24,
                attack: AttackValue::Two,
                ballistic_skill: 3,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            RangedWeapon::Flamer => RangedWeaponStats {
                range: 12,
                attack: AttackValue::D6,
                ballistic_skill: 0, // torrent weapon so always hits
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            RangedWeapon::MissleLauncherFrag => RangedWeaponStats {
                range: 48,
                attack: AttackValue::D6,
                ballistic_skill: 4,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
            RangedWeapon::GaussFlayer => RangedWeaponStats {
                range: 24,
                attack: AttackValue::One,
                ballistic_skill: 4,
                strength: 4,
                armor_penetration: 0,
                damage: 1,
            },
        }
    }
}

impl Display for RangedWeapon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use RangedWeapon::*;
        f.write_str(match self {
            BoltPistol => "Bolt pistol",
            Boltgun => "Boltgun",
            Flamer => "Flamer",
            MissleLauncherFrag => "Missle Launcher - Frag",
            GaussFlayer => "Gauss Flayer",
        })
    }
}
