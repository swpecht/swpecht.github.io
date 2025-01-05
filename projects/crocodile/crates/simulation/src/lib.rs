#![feature(let_chains)]

use bevy::prelude::Component;

pub mod gamestate;
pub mod info;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Copy, Component)]
pub enum ModelSprite {
    Skeleton,
    Knight,
    Orc,
    Wizard,
}

impl ModelSprite {
    pub fn asset_loc(&self) -> &str {
        match self {
            ModelSprite::Skeleton => "pixel-crawler/Enemy/Skeleton Crew/Skeleton - Base",
            ModelSprite::Knight => "pixel-crawler/Heroes/Knight",
            ModelSprite::Orc => "pixel-crawler/Enemy/Orc Crew/Orc",
            ModelSprite::Wizard => "pixel-crawler/Heroes/Wizard",
        }
    }
}
