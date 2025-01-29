use bevy::{math::vec3, prelude::*};

use crate::hex::coords_to_pixel;

use super::{GRID_HEIGHT, GRID_WIDTH, TILE_LAYER, TILE_SIZE};

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

pub fn setup_tiles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    // https://www.redblobgames.com/grids/hexagons/#coordinates

    let shape = meshes.add(RegularPolygon::new(TILE_SIZE as f32 / 2.0 - 1., 6));
    let color = Color::srgb(0.0, 1.0, 0.0);

    for r in 0..GRID_WIDTH + 1 {
        for c in 0..GRID_HEIGHT + 1 {
            let pixel = coords_to_pixel(r, c);
            commands.spawn((
                Mesh2d(shape.clone()),
                MeshMaterial2d(materials.add(color)),
                // Text2d::new(format!("{}, {}", r, c)),
                Transform {
                    translation: vec3(pixel.x as f32, pixel.y as f32, TILE_LAYER),
                    ..default()
                },
            ));
        }
    }
}
