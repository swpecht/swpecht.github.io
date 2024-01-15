use card_platypus::{
    algorithms::cfres::{DepthChecker, EuchreDepthChecker},
    database::NodeStore,
};
use games::{
    gamestates::euchre::{actions::EAction, Euchre, EuchreGameState},
    translate_istate, GameState,
};
use rand::{seq::SliceRandom, thread_rng};

#[test]
fn test_euchre_indexing() {
    let indexer = card_platypus::database::indexer::Indexer::euchre();

    let depth_checker = EuchreDepthChecker {
        max_cards_played: 4,
    };

    let mut actions = Vec::new();
    let mut rng = thread_rng();

    for _ in 0..1_000_000 {
        let mut gs = Euchre::new_state();

        while !(gs.is_terminal() || depth_checker.is_max_depth(&gs)) {
            if !gs.is_chance_node() {
                let key = gs.istate_key(gs.cur_player());
                indexer.index(&key).unwrap_or_else(|| {
                    panic!("failed to index: {:?}", translate_istate!(key, EAction))
                });
            }

            gs.legal_actions(&mut actions);
            let a = actions.choose(&mut rng).unwrap();
            gs.apply_action(*a);
        }
    }
}
