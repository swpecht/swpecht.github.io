use std::fmt::Display;

use crate::{gamestate::Stats, ui::sprite::CharacterSprite};

/// Pre-built characters
#[derive(Debug)]
pub enum PreBuiltCharacter {
    Skeleton,
    Knight,
    HumanSoldier,
    WarMagicWizard,
    GiantGoat,
    FemaleSteeder,
}

pub enum Effect {
    /// Moves self to the square next to the target
    Charge,
    KnockDown,
}

/// The "Actions" a character can take in D&D terminology
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Ability {
    #[default]
    MeleeAttack,
    BowAttack,
    Longsword,
    LightCrossbow,
    Charge,
    Ram,
}

impl PreBuiltCharacter {
    pub fn sprite(&self) -> CharacterSprite {
        use PreBuiltCharacter::*;
        match self {
            Skeleton => CharacterSprite::Skeleton,
            Knight => CharacterSprite::Knight,
            HumanSoldier => CharacterSprite::Knight,
            WarMagicWizard => CharacterSprite::Wizard,
            GiantGoat => CharacterSprite::Skeleton,
            FemaleSteeder => CharacterSprite::Orc,
        }
    }

    pub fn stats(&self) -> Stats {
        use PreBuiltCharacter::*;
        match self {
            Skeleton => Stats {
                health: 10,
                str: 17,
                dex: 11,
                con: 12,
                int: 3,
                wis: 12,
                cha: 6,
                ac: 11,
                movement: 8,
            },
            Knight => Stats {
                health: 15,
                str: 17,
                dex: 11,
                con: 12,
                int: 3,
                wis: 12,
                cha: 6,
                ac: 11,
                movement: 8,
            },
            HumanSoldier => Stats {
                health: 28,
                str: 16,
                dex: 14,
                con: 15,
                int: 9,
                wis: 13,
                cha: 11,
                ac: 18,
                movement: 6,
            },
            GiantGoat => Stats {
                health: 19,
                str: 17,
                dex: 11,
                con: 12,
                int: 3,
                wis: 12,
                cha: 6,
                ac: 11,
                movement: 8,
            },

            _ => panic!("not implemented for: {:?}", self),
        }
    }

    pub fn abilities(&self) -> Vec<Ability> {
        use PreBuiltCharacter::*;
        match self {
            Skeleton => vec![Ability::MeleeAttack],
            Knight => vec![Ability::MeleeAttack, Ability::BowAttack],
            HumanSoldier => vec![Ability::MeleeAttack, Ability::BowAttack],
            GiantGoat => vec![Ability::Ram, Ability::Charge],
            _ => panic!("not implemented for: {:?}", self),
        }
    }
}

impl Ability {
    pub fn max_range(&self) -> u8 {
        use Ability::*;
        match self {
            MeleeAttack | Longsword | Ram => 1,
            Charge => 4,
            LightCrossbow => 16,
            BowAttack => 20,
        }
    }

    pub fn min_range(&self) -> u8 {
        use Ability::*;
        match self {
            Charge => 4,
            _ => 1, // By default can cast to all targets other than self
        }
    }

    pub fn dmg(&self) -> u8 {
        use Ability::*;
        match self {
            MeleeAttack => 5,
            BowAttack => 2,
            Longsword => 8,
            LightCrossbow => 6,
            Charge => 13,
            Ram => 8,
        }
    }

    pub fn to_hit(&self) -> u8 {
        use Ability::*;
        match self {
            MeleeAttack => 5,
            BowAttack => 5,
            Longsword => 5,
            LightCrossbow => 4,
            Charge | Ram => 5,
        }
    }
}

impl Display for Ability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use Ability::*;
        f.write_str(match self {
            MeleeAttack => "Melee",
            BowAttack => "Bow",
            Longsword => "LongSword",
            LightCrossbow => "LightCrossbow",
            Charge => "Charge",
            Ram => "Ram",
        })
    }
}

// ##### Testing specs #####
// # DO NOT EDIT or tests need to be updated

// Skeleton:
//   sprite: "Skeleton"
//   stats:
//     health: 10
//     str: 17
//     dex: 11
//     con: 12
//     int: 3
//     wis: 12
//     cha: 6
//     ac: 11
//     movement: 8
//   actions:
//     Melee:
//       max_range: 1
//       damage: 5
//       to_hit: 5

// Knight:
//   sprite: "Knight"
//   stats:
//     health: 15
//     str: 17
//     dex: 11
//     con: 12
//     int: 3
//     wis: 12
//     cha: 6
//     ac: 11
//     movement: 8
//   actions:
//     Melee:
//       max_range: 1
//       damage: 5
//       to_hit: 5
//     Bow:
//       max_range: 8
//       damage: 2
//       to_hit: 5

// ####### Game specs #######
// # https://joe.nittoly.ca/wordpress/wp-content/uploads/2021/04/DD-5e-Fighter-3-Champion-Human-Soldier.pdf
// Human soldier:
//   sprite: "Knight"
//   stats:
//     health: 28
//     str: 16
//     dex: 14
//     con: 15
//     int: 9
//     wis: 13
//     cha: 11
//     ac: 18
//     movement: 6
//   actions:
//     Longsword:
//       max_range: 1
//       damage: 8
//       to_hit: 5
//     Light Crossbow:
//       max_range: 16
//       damage: 6
//       to_hit: 4
//   bonus_actions:
//     Second Wind:
//       max_range: 0
//       damage: -8
//       reset: "short_rest"
//     Action Surge:
//       max_range: 0
//       addtl_action: 1
//       reset: "short_rest"
//   # passives:
//   #   Improved Critical:

// # https://joe.nittoly.ca/wordpress/wp-content/uploads/2021/04/DD-5e-Wizard-3-War-Magic-Dark-Elf-Mercenary-Veteran.pdf
// War Magic Wizard:
//   sprite: "Wizard"
//   stats:
//     health: 20
//     str: 10
//     dex: 12
//     con: 14
//     int: 15
//     wis: 10
//     cha: 14
//     ac: 11
//     movement: 6
//   actions:
//     # Farie Fire:
//     # Blade Ward:
//     Fire Bolt:
//       max_range: 24
//       damage: 5
//       to_hit: 5 # TBD
//       # figure out chance to hit for spells
//     # True Strike:
//     #   max_range: 6
//     # Invisibility:
//     # Scorching Ray:

// Giant goat:
//   sprite: "Skeleton"
//   stats:
//     health: 19
//     str: 17
//     dex: 11
//     con: 12
//     int: 3
//     wis: 12
//     cha: 6
//     ac: 11
//     movement: 8
//   actions:
//     Ram:
//       max_range: 1
//       damage: 8
//       to_hit: 5
//     Charge:
//       min_range: 4
//       max_range: 4
//       damage: 13
//       to_hit: 5
//       effects:
//         knock down:
//           dc: 13
//           type: "str"
//           effect: "Prone"
//         # todo: figure out the movement part

// Female Steeder:
//   sprite: "Orc"
//   stats:
//     health: 30
//     str: 15
//     dex: 16
//     con: 14
//     int: 2
//     wis: 10
//     cha: 3
//     ac: 14
//     movement: 6
//   actions:
//     Bite:
//       max_range: 1
//       damage: 7
//       to_hit: 5
//       effects:
//         acid:
//           dc: 12
//           type: "con"
//           dmg_fail: 9
//           dmg_succede: 4
//       Sticky Leg:
//         max_range: 1
//         to_hit: 20 # always hit since applies effect
//         effects:
//           web:
//             effect: "Grappled"
//             dc: 12
//             type: "str"
//             immediate: false
//   bonus_actions:
//     Leap:
//       min_movement: 6
//       range: 18
