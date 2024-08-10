use std::path::Path;

use bevy::utils::HashMap;
use serde::Deserialize;

use crate::{gamestate::Stats, ui::sprite::CharacterSprite};

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterSpec {
    pub sprite: CharacterSprite,
    pub stats: Stats,
    actions: HashMap<String, ActionStats>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ActionStats {
    max_range: u8,
    // min_range: u8,
    damage: u8,
    to_hit: u8,
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
