use std::collections::HashSet;

use card_platypus::algorithms::cfres::{DepthChecker, EuchreDepthChecker};
use games::{
    gamestates::euchre::{
        actions::EAction, isomorphic::normalize_euchre_istate,
        iterator::EuchreIsomorphicIStateIterator, Euchre,
    },
    istate::IStateKey,
    translate_istate, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

#[test]
fn test_euchre_indexing() {
    run_euchre_indexing(1, 1_000_000);
}

#[test]
#[ignore]
fn test_euchre_indexing_max2() {
    run_euchre_indexing(2, 100_000);
}

#[test]
#[ignore]
fn test_euchre_indexing_max3() {
    run_euchre_indexing(3, 10_000);
}

/// Quick smoke test: build the MPHF at max=3 via the production path and
/// verify a few thousand CFR random walks don't hit a definitive `None` from
/// `try_hash`. Catches missing istates (hard failures) without materializing
/// the full iterator output in memory.
#[test]
#[ignore]
fn smoke_mphf_max3() {
    use card_platypus::algorithms::cfres::{DepthChecker, EuchreDepthChecker};

    let max_cards_played = 3;
    eprintln!("building indexer...");
    let indexer = card_platypus::database::indexer::Indexer::euchre(max_cards_played);
    eprintln!("indexer built; shard_len={}", indexer.len() / 6);

    let depth_checker = EuchreDepthChecker { max_cards_played };
    let mut actions = Vec::new();
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut none_count = 0;
    let mut total = 0;
    let mut none_examples: Vec<IStateKey> = Vec::new();
    for _ in 0..10_000 {
        let mut gs = Euchre::new_state();
        while !(gs.is_terminal() || depth_checker.is_max_depth(&gs)) {
            if !gs.is_chance_node() {
                let key = gs.istate_key(gs.cur_player());
                total += 1;
                if indexer.index(&key).is_none() {
                    none_count += 1;
                    if none_examples.len() < 5 {
                        none_examples.push(normalize_euchre_istate(&key));
                    }
                }
            }
            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }

    if none_count > 0 {
        for (i, k) in none_examples.iter().enumerate() {
            eprintln!("  None #{}: {:?}", i, translate_istate!(*k, EAction));
        }
        panic!(
            "max=3 smoke: {} queries returned None from indexer out of {}",
            none_count, total
        );
    }
    eprintln!("max=3 smoke: {} queries, all indexable", total);
}

/// Diagnostic: count CFR-queried istates split by phase and "has discard visible"
/// to understand the phantom ratio.
#[test]
#[ignore]
fn diag_cfr_query_distribution_max2() {
    use card_platypus::algorithms::cfres::{DepthChecker, EuchreDepthChecker};
    use games::gamestates::euchre::actions::EAction;

    let max_cards_played = 2;
    let depth_checker = EuchreDepthChecker { max_cards_played };

    let mut actions = Vec::new();
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut by_phase = std::collections::HashMap::<&'static str, usize>::new();
    let mut unique: HashSet<IStateKey> = HashSet::new();

    for _ in 0..100_000 {
        let mut gs = Euchre::new_state();
        while !(gs.is_terminal() || depth_checker.is_max_depth(&gs)) {
            if !gs.is_chance_node() {
                let player = gs.cur_player();
                let key = gs.istate_key(player);
                let normed = normalize_euchre_istate(&key);
                unique.insert(normed);

                let last = key.iter().last().copied().map(EAction::from);
                let has_discard = key
                    .iter()
                    .map(|a| EAction::from(*a))
                    .any(|a| matches!(a, EAction::Pickup))
                    && player == 3;
                let phase_label = match (gs.phase(), has_discard, last) {
                    (games::gamestates::euchre::EPhase::Play, true, _) => "play_dealer_view",
                    (games::gamestates::euchre::EPhase::Play, false, _) => "play_other_view",
                    (games::gamestates::euchre::EPhase::Alone, _, _) => "alone",
                    (games::gamestates::euchre::EPhase::Discard, _, _) => "discard",
                    (games::gamestates::euchre::EPhase::ChooseTrump, _, _) => "choose_trump",
                    (games::gamestates::euchre::EPhase::Pickup, _, _) => "pickup",
                    _ => "other",
                };
                *by_phase.entry(phase_label).or_default() += 1;
            }

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }

    eprintln!("CFR-query distribution at max_cards_played=2:");
    let mut entries: Vec<_> = by_phase.iter().collect();
    entries.sort_by_key(|(_, v)| std::cmp::Reverse(**v));
    for (k, v) in entries {
        eprintln!("  {:25} {}", k, v);
    }
    eprintln!("  unique normalized istates seen: {}", unique.len());
}

fn run_euchre_indexing(max_cards_played: usize, iterations: usize) {
    // Build the ground-truth set of istates the iterator emits. This is what
    // the MPHF is built from. CFR-queried istates must be a subset of this.
    let iterator_keys: HashSet<IStateKey> =
        EuchreIsomorphicIStateIterator::new(max_cards_played).collect();
    eprintln!(
        "iterator emitted {} unique istates for max_cards_played={}",
        iterator_keys.len(),
        max_cards_played
    );

    let depth_checker = EuchreDepthChecker { max_cards_played };

    let mut actions = Vec::new();
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut successful_lookups = 0;
    let mut missing_examples: Vec<IStateKey> = Vec::new();
    let mut missing_count = 0;
    for _ in 0..iterations {
        let mut gs = Euchre::new_state();

        while !(gs.is_terminal() || depth_checker.is_max_depth(&gs)) {
            if !gs.is_chance_node() {
                let key = gs.istate_key(gs.cur_player());
                let normed = normalize_euchre_istate(&key);
                if iterator_keys.contains(&normed) {
                    successful_lookups += 1;
                } else {
                    missing_count += 1;
                    if missing_examples.len() < 5 {
                        missing_examples.push(normed);
                    }
                }
            }

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }

    if missing_count > 0 {
        for (i, k) in missing_examples.iter().enumerate() {
            eprintln!(
                "  missing #{}: {:?}",
                i,
                translate_istate!(*k, EAction)
            );
        }
        panic!(
            "max_cards_played={}: {} CFR-queried istates were missing from iterator (out of {} total queries)",
            max_cards_played,
            missing_count,
            successful_lookups + missing_count,
        );
    }
}
