use card_platypus::algorithms::cfres::{DepthChecker, EuchreDepthChecker};
use games::{
    gamestates::euchre::{actions::EAction, isomorphic::normalize_euchre_istate, Euchre},
    translate_istate, GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

/// Smoke test: random-walk CFR-style traversal must never hit an istate the
/// indexer can't find a slot for. Uses the production indexer path (MPHF
/// `try_hash`), which catches missing istates as definitive `None` results
/// but is not a strict set-membership check.
#[test]
fn test_euchre_indexing() {
    run_euchre_indexing(1, 100_000);
}

fn run_euchre_indexing(max_cards_played: usize, iterations: usize) {
    let indexer = card_platypus::database::indexer::Indexer::euchre(max_cards_played);
    let depth_checker = EuchreDepthChecker { max_cards_played };

    let mut actions = Vec::new();
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut successful = 0;
    for _ in 0..iterations {
        let mut gs = Euchre::new_state();

        while !(gs.is_terminal() || depth_checker.is_max_depth(&gs)) {
            if !gs.is_chance_node() {
                let key = gs.istate_key(gs.cur_player());
                indexer.index(&key).unwrap_or_else(|| {
                    let normed = normalize_euchre_istate(&key);
                    panic!(
                        "failed to index after {} successful (max={})\n\tistate: {:?}\n\tnormed: {:?}",
                        successful,
                        max_cards_played,
                        translate_istate!(key, EAction),
                        translate_istate!(normed, EAction)
                    )
                });
                successful += 1;
            }

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }
}
