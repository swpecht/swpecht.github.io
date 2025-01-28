use core::todo;

use bevy::{math::vec3, prelude::*};

use crate::hex::{hex_to_pixel, layout_flat, layout_pointy, Hex, Layout, Point};

use super::{GRID_HEIGHT, GRID_WIDTH, TILE_LAYER, TILE_SIZE};

const QUADRANT_SIDE_LENGTH: u32 = 80;

#[derive(Component)]
struct GameTile;

pub fn setup_tiles_square(
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

pub fn setup_tiles(mut commands: Commands, asset_server: Res<AssetServer>) {
    let tile: Handle<Image> = asset_server.load("tiles/mars_04.png");
    // https://www.redblobgames.com/grids/hexagons/#coordinates

    let w = 120.0;
    let h = 140.0;

    let target_width = 64.;

    for r in 0..GRID_WIDTH + 1 {
        for c in 0..GRID_HEIGHT + 1 {
            let layout = Layout {
                orientation: layout_pointy,
                size: Point {
                    x: w / 2.0,
                    y: h / 2.0,
                },
                origin: Point { x: 0.0, y: 0.0 },
            };

            let hex = Hex::new(r, c);
            let pixel = hex_to_pixel(layout, hex);
            commands.spawn((
                Sprite::from_image(tile.clone()),
                Transform {
                    translation: vec3(pixel.x as f32, pixel.y as f32, TILE_LAYER),
                    scale: vec3((target_width / w) as f32, (target_width / h) as f32, 1.),
                    ..default()
                },
            ));
        }
    }
}
