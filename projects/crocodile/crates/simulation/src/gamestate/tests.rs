use core::{assert, assert_eq};

use rand::{rngs::StdRng, SeedableRng};

use super::*;

#[test]
fn test_charge_phase() {
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(2, 10)], Team::Players);
    insert_space_marine_unit(&mut gs, vec![sc(1, 5)], Team::Players);
    insert_necron_unit(&mut gs, vec![sc(1, 15), sc(1, 16), sc(2, 16)], Team::NPCs);
    gs.set_phase(Phase::Charge, Team::Players);
    assert!(gs.is_chance_node());

    gs.apply(Action::RollResult { num_success: 4 });
    gs.apply(Action::RollResult { num_success: 4 });

    let mut actions = Vec::new();
    gs.legal_actions(&mut actions);
    use Action::*;
    assert_eq!(
        actions,
        vec![
            EndPhase,
            Charge {
                id: ModelId(0),
                from: SimCoords { x: 1, y: 10 },
                to: SimCoords { x: 1, y: 14 }
            },
        ]
    );

    gs.apply(Charge {
        id: ModelId(0),
        from: SimCoords { x: 1, y: 10 },
        to: SimCoords { x: 1, y: 14 },
    });

    gs.legal_actions(&mut actions);

    // todo: fix this
    assert_eq!(
        actions,
        vec![
            RemoveModel { id: ModelId(0) },
            RemoveModel { id: ModelId(1) },
            Charge {
                id: ModelId(1),
                from: SimCoords { x: 2, y: 10 },
                to: SimCoords { x: 1, y: 13 }
            },
            Charge {
                id: ModelId(1),
                from: SimCoords { x: 2, y: 10 },
                to: SimCoords { x: 2, y: 14 }
            }
        ]
    );
}

#[test]
fn test_unit_coherency() {
    // Single model units are coherent
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
    assert!(gs.unit_coherency().iter().all(|x| x.1));

    // Models in a straight line don't have coherency as swarms
    let mut gs = SimState::new();
    insert_space_marine_unit(
        &mut gs,
        (0..10).map(|x| sc(1 + x, 10)).collect_vec(),
        Team::Players,
    );
    assert!(!gs.unit_coherency().iter().all(|x| x.1));

    // But non-swarm units will
    let mut gs = SimState::new();
    insert_space_marine_unit(
        &mut gs,
        (0..5).map(|i| sc(1 + i, 5)).collect_vec(),
        Team::Players,
    );
    assert!(gs.unit_coherency().iter().all(|x| x.1));

    // Non-swarm aren't coherent with a gap
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(3, 10)], Team::Players);
    assert!(
        !gs.unit_coherency().iter().all(|x| x.1),
        "Non-swarm aren't coherent with a gap"
    );

    // Swarm are coherent in a rectangle
    let mut gs = SimState::new();
    insert_space_marine_unit(
        &mut gs,
        (0..20).map(|i| sc(1 + i % 10, 5 + i / 10)).collect_vec(),
        Team::Players,
    );
    assert!(gs.unit_coherency().iter().all(|x| x.1));

    // enemy units don't count for coherency
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(3, 10)], Team::Players);
    insert_space_marine_unit(&mut gs, vec![sc(2, 10)], Team::NPCs);
    assert_eq!(gs.unit_coherency().iter().filter(|x| !x.1).count(), 2);

    // player models but different units don't count for coherency
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10), sc(3, 10)], Team::Players);
    insert_space_marine_unit(&mut gs, vec![sc(2, 10)], Team::Players);
    assert_eq!(gs.unit_coherency().iter().filter(|x| !x.1).count(), 2);

    // All units in a unit must have a path between them, e.g. can't have two groups
    let mut gs = SimState::new();
    insert_space_marine_unit(
        &mut gs,
        vec![sc(1, 10), sc(2, 10), sc(1, 12), sc(2, 12)],
        Team::Players,
    );
    assert!(!gs.unit_coherency().iter().all(|x| x.1));
    let mut actions = Vec::new();
    gs.legal_actions(&mut actions);
    assert!(
        !actions.contains(&Action::EndPhase),
        "Can't end turn when not in unit coherency"
    );

    // Removing a unit should fix unit coherency
    let mut gs = SimState::new();
    insert_space_marine_unit(
        &mut gs,
        vec![sc(1, 10), sc(2, 10), sc(4, 10)],
        Team::Players,
    );

    assert!(!gs.unit_coherency().iter().all(|x| x.1));
    gs.apply(Action::RemoveModel { id: ModelId(2) });
    assert!(gs.unit_coherency().iter().all(|x| x.1));

    // Coherency works with multiple units and teams
    let mut gs = SimState::new();
    // insert_space_marine_unit(&mut state, sc(5, 10), Team::Players, 0, 10);
    insert_space_marine_unit(
        &mut gs,
        vec![sc(1, 10), sc(2, 10), sc(3, 10)],
        Team::Players,
    );
    insert_necron_unit(&mut gs, vec![sc(1, 15), sc(2, 15), sc(3, 15)], Team::NPCs);
    assert!(gs.unit_coherency().iter().all(|x| x.1));
}

#[test]
fn test_phase_change() {
    let mut gs = SimState::new();
    assert_eq!(gs.phase(), Phase::Movement); // for now starting in movement phase
    assert_eq!(gs.cur_team(), Team::Players);
    gs.apply(Action::EndPhase);
    assert_eq!(gs.phase(), Phase::Shooting);
    assert_eq!(gs.cur_team(), Team::Players);
    gs.apply(Action::EndPhase);
    assert_eq!(gs.phase(), Phase::Charge);
    assert_eq!(gs.cur_team(), Team::Players);
    gs.apply(Action::EndPhase);
    assert_eq!(gs.phase(), Phase::Fight);
    assert_eq!(gs.cur_team(), Team::Players);
    gs.apply(Action::EndPhase);
    assert_eq!(gs.phase(), Phase::Command);
    assert_eq!(gs.cur_team(), Team::NPCs);
}

#[test]
fn test_set_phase() {
    let mut gs = SimState::new();
    gs.set_phase(Phase::Fight, Team::NPCs);
    assert_eq!(gs.phase(), Phase::Fight);
    assert_eq!(gs.cur_team(), Team::NPCs);
}

#[test]
fn test_shooting_legal_actions() {
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
    gs.set_phase(Phase::Shooting, Team::Players);
    let mut actions = Vec::new();

    // no targets
    gs.legal_actions(&mut actions);
    assert_eq!(actions, vec![Action::EndPhase]);

    // single target out of range
    insert_necron_unit(&mut gs, vec![sc(50, 50)], Team::NPCs);
    gs.legal_actions(&mut actions);
    assert_eq!(actions, vec![Action::EndPhase]);

    // single target in range
    insert_necron_unit(&mut gs, vec![sc(3, 10), sc(4, 10)], Team::NPCs);
    gs.legal_actions(&mut actions);
    assert_eq!(
        actions,
        vec![
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(3),
                ranged_weapon: Weapon::BoltPistol
            },
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(3),
                ranged_weapon: Weapon::Boltgun
            },
            Action::EndPhase
        ]
    );

    // add in when part of the unit is in range and part is out of range, on both the attacking a fired upon units
    insert_necron_unit(
        &mut gs,
        vec![sc((1 + Weapon::BoltPistol.stats().range + 1).into(), 10)],
        Team::NPCs,
    );
    gs.legal_actions(&mut actions);
    assert_eq!(
        actions,
        vec![
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(3),
                ranged_weapon: Weapon::BoltPistol
            },
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(3),
                ranged_weapon: Weapon::Boltgun
            },
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(4),
                ranged_weapon: Weapon::Boltgun
            },
            Action::EndPhase
        ]
    );
}

#[test]
fn test_shoot_phase() {
    let mut gs = SimState::new();
    insert_space_marine_unit(&mut gs, vec![sc(1, 10)], Team::Players);
    insert_necron_unit(&mut gs, vec![sc(3, 10), sc(4, 10)], Team::NPCs);
    gs.set_phase(Phase::Shooting, Team::Players);

    assert_eq!(
        unit_models!(gs, UnitId(2))
            .map(|m| m.cur_stats.wound)
            .sum::<u8>(),
        2
    );

    let mut actions = Vec::new();
    gs.legal_actions(&mut actions);
    // Can't use melee weapons in shooting phase
    assert_eq!(
        actions,
        vec![
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(2),
                ranged_weapon: Weapon::BoltPistol,
            },
            Action::Shoot {
                from: UnitId(1),
                to: UnitId(2),
                ranged_weapon: Weapon::Boltgun,
            },
            Action::EndPhase,
        ]
    );

    gs.set_phase(Phase::Shooting, Team::Players);
    gs.apply(Action::Shoot {
        from: UnitId(1),
        to: UnitId(2),
        ranged_weapon: Weapon::Boltgun,
    });

    let mut actions = Vec::new();
    gs.legal_actions(&mut actions);
    assert_eq!(actions, vec![]);

    assert!(gs.is_chance_node());
    let probs = gs.chance_outcomes();
    let mut rng: StdRng = SeedableRng::seed_from_u64(43);
    let a = probs.sample(&mut rng);
    // should be one success from seeded rng
    assert!(matches!(a, Action::RollResult { num_success: 1 }));
    gs.apply(a);

    assert!(!gs.is_chance_node());

    // Should have 1 wound, the extra damage from the boltrifle doesn't spill over
    assert_eq!(
        unit_models!(gs, UnitId(2))
            .map(|m| m.cur_stats.wound)
            .sum::<u8>(),
        1
    );
}

#[test]
fn test_undo() {
    let mut start_state = SimState::new();
    insert_space_marine_unit(
        &mut start_state,
        (0..10).map(|i| sc(1 + i, 10)).collect_vec(),
        Team::Players,
    );
    insert_space_marine_unit(
        &mut start_state,
        (0..10).map(|i| sc(1 + i, 15)).collect_vec(),
        Team::NPCs,
    );

    let mut rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut actions = Vec::new();
    let mut index = 0;

    for _ in 0..1000 {
        // times to run the test
        let mut state = start_state.clone();
        for _ in 0..100 {
            // max number of generations
            if state.is_terminal() {
                break;
            }

            let undo_state = state.clone();
            let a = if state.is_chance_node() {
                let probs = state.chance_outcomes();
                probs.sample(&mut rng)
            } else {
                state.legal_actions(&mut actions);
                use rand::prelude::SliceRandom;
                *actions.choose(&mut rng).unwrap()
            };

            state.apply(a);
            state.undo();
            assert_eq!(
                state,
                undo_state,
                "failed to undo index {}: {:?}\n{:#?}",
                index,
                a,
                state._diff_between(&undo_state)
            );
            state.apply(a);
            index += 1;
        }
    }
}
