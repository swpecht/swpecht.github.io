use std::{
    hash::{DefaultHasher, Hasher},
    path::Path,
};

use bevy::utils::HashMap;
use serde::Deserialize;

use crate::gamestate::Stats;

pub type CharacterId = u8;

#[derive(Debug, Clone, Deserialize)]
#[serde(from = "CharacterSpecSerde")]
pub struct CharacterSpec {
    pub id: CharacterId,
    pub art: String,
    pub stats: Stats,
}

#[derive(Deserialize)]
struct CharacterSpecSerde {
    pub art: String,
    pub stats: Stats,
}

impl From<CharacterSpecSerde> for CharacterSpec {
    fn from(value: CharacterSpecSerde) -> Self {
        let CharacterSpecSerde { art, stats } = value;
        let mut hasher = DefaultHasher::new();
        std::hash::Hash::hash(&art, &mut hasher);

        CharacterSpec {
            id: hasher.finish() as CharacterId,
            art,
            stats,
        }
    }
}

pub fn load_encounter(path: &Path) -> anyhow::Result<HashMap<String, CharacterSpec>> {
    let str = std::fs::read_to_string(path)?;
    let characters = serde_yaml::from_str(&str)?;
    Ok(characters)
}

#[macro_export]
macro_rules! load_spec {
    ($character_name:expr) => {{
        let characters = $crate::parser::load_encounter(std::path::Path::new("encounter.yaml"))
            .expect("error loading encounter");
        let spec = characters
            .get($character_name)
            .unwrap_or_else(|| panic!("failed to load character: {}", $character_name))
            .clone();
        spec
    }};
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::load_encounter;

    #[test]
    fn test_load_encounter() {
        let characters =
            load_encounter(Path::new("encounter.yaml")).expect("error loading encounter");
        print!("{:#?}", characters);
    }
}
