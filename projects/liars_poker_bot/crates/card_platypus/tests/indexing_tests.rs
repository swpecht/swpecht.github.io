use card_platypus::algorithms::cfres::{DepthChecker, EuchreDepthChecker};
use games::{
    gamestates::euchre::{actions::EAction, ismorphic::normalize_euchre_istate, Euchre},
    translate_istate, GameState,
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

#[test]
fn test_euchre_indexing() {
    let max_cards_played = 2;
    let indexer = card_platypus::database::indexer::Indexer::euchre(max_cards_played);

    let depth_checker = EuchreDepthChecker { max_cards_played };

    let mut actions = Vec::new();
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut successful_indexes = 0;
    for _ in 0..1_000_000 {
        let mut gs = Euchre::new_state();

        while !(gs.is_terminal() || depth_checker.is_max_depth(&gs)) {
            if !gs.is_chance_node() {
                let key = gs.istate_key(gs.cur_player());
                indexer.index(&key).unwrap_or_else(|| {
                    let normed = normalize_euchre_istate(&key);
                    panic!(
                        "failed to index after {} successful\n\tistate: {:?}\n\tnormed: {:?}",
                        successful_indexes,
                        translate_istate!(key, EAction),
                        translate_istate!(normed, EAction)
                    )
                });
                successful_indexes += 1;
            }

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }
}
