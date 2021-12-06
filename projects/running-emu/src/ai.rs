use hecs::World;

use crate::{Attack, AttackerAgent, Damage, Health, Position};

/// Have towers attack units in range
pub fn system_defense_ai(world: &mut World) {
    let mut targets = Vec::new();

    for (e, (pos, _, _)) in world.query_mut::<(&Position, &AttackerAgent, &Health)>() {
        targets.push((e, pos.0));
    }

    let mut attacks = Vec::new();
    for (e, (pos, attack)) in world
        .query_mut::<(&Position, &Attack)>()
        .without::<AttackerAgent>()
    {
        for (target, target_pos) in &targets {
            if pos.0.dist(&target_pos) <= attack.range as i32 {
                attacks.push((
                    target,
                    Damage {
                        amount: attack.damage,
                        from: e,
                    },
                ));
                continue; // Only one attack per tick
            }
        }
    }

    for (target, damage) in attacks {
        world.insert_one(*target, damage).unwrap();
    }
}
