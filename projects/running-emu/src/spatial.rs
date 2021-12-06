use hecs::{Entity, EntityBuilder, World};
use std::hash::Hash;

use crate::{get_max_point, Position};

/// Read only cache for spatial based lookups.
pub struct SpatialCache {
    entity_lookup: Vec<Vec<Vec<Entity>>>,
}

impl SpatialCache {
    pub fn new(world: &World) -> Self {
        let max_p = get_max_point(world);
        let entity_lookup = vec![vec![Vec::new(); max_p.x]; max_p.y];

        let mut c = Self { entity_lookup };
        c.populate_entity_lookup(world);
        return c;
    }

    fn populate_entity_lookup(&mut self, world: &World) {
        for (id, pos) in world.query::<&Position>().into_iter() {
            self.entity_lookup[pos.0.y][pos.0.x].push(id);
        }
    }

    /// Returns the tile at a given location
    pub fn get_entities(&self, point: Point) -> Vec<Entity> {
        return self.entity_lookup[point.y][point.x].clone();
    }
}

pub fn system_update_spatial_cache(world: &mut World) {
    let cache;

    for (_, c) in world.query::<&mut SpatialCache>().iter() {
        cache = c;
        // Clear the chache
        let width = cache.entity_lookup[0].len();
        let height = cache.entity_lookup.len();
        for y in 0..height {
            for x in 0..width {
                cache.entity_lookup[y][x].clear();
            }
        }
        cache.populate_entity_lookup(world);

        return;
    }

    let cache = SpatialCache::new(world);
    let mut builder = EntityBuilder::new();
    builder.add(cache);
    // Otherwise we create one if it doesn't exist
    world.spawn(builder.build());
}

pub fn get_entities(world: &World, p: Point) -> Vec<Entity> {
    for (_, cache) in world.query::<&SpatialCache>().iter() {
        return cache.get_entities(p);
    }

    // If no cache, need to iterate and find
    let mut positions = world.query::<&Position>();
    let mut results = Vec::new();
    for (id, candidate) in positions.iter() {
        if candidate.0 == p {
            results.push(id)
        }
    }
    return results;
}

/// Point in the game world
#[derive(PartialEq, Clone, Copy, Hash, Eq, Debug)]
pub struct Point {
    pub x: usize,
    pub y: usize,
}

impl Point {
    /// Returns taxicab distance between points
    pub fn dist(&self, p: &Point) -> i32 {
        return (self.x as i32 - p.x as i32).abs() + (self.y as i32 - p.y as i32).abs();
    }
}

pub fn print_path(path: &Vec<Point>, world: &World) {
    let max_p = get_max_point(world);
    for y in 0..max_p.x {
        for x in 0..max_p.y {
            let p = Point { x: x, y: y };
            if path.contains(&p) {
                print!("#")
            } else {
                print! {"."}
            }
        }
        println!("")
    }
}
