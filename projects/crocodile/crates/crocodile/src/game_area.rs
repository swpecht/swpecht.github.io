use bevy::{math::vec3, prelude::*};

use super::{GRID_HEIGHT, GRID_WIDTH, TILE_LAYER, TILE_SIZE};

#[derive(Component)]
struct GameTile;

pub fn setup_tiles(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    // let texture = asset_server.load("pixel-crawler/Environment/Green Woods/Assets/Tiles.png");
    let texture = asset_server.load("tiles/grass.png");
    let layout = TextureAtlasLayout::from_grid(UVec2::new(32, 32), 4, 4, None, None);
    let texture_atlas_layout = texture_atlas_layouts.add(layout);

    for r in 0..GRID_WIDTH + 1 {
        for c in 0..GRID_HEIGHT + 1 {
            commands.spawn((
                GameTile,
                Sprite::from_atlas_image(
                    texture.clone(),
                    TextureAtlas {
                        layout: texture_atlas_layout.clone(),
                        index: 5,
                    },
                ),
                Transform::from_translation(vec3(
                    (r * TILE_SIZE) as f32,
                    (c * TILE_SIZE) as f32,
                    TILE_LAYER,
                )),
            ));
        }
    }
}
