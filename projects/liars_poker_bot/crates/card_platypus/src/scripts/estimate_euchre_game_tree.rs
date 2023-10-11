use card_platypus::agents::{Agent, RandomAgent};
use games::{actions, gamestates::euchre::Euchre, GameState};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{Args, GameType};

pub fn estimate_euchre_game_tree(args: Args) {
    assert_eq!(args.game, GameType::Euchre);

    let mut total_end_states = 0;
    let mut total_states = 0;
    let mut _total_rounds = 0;
    let mut children = [0.0; 28];
    let runs = 10000;
    let mut agent = RandomAgent::new();

    for _ in 0..runs {
        let mut round = 0;
        let mut end_states = 1;
        let mut gs = Euchre::new_state();
        while !gs.is_terminal() {
            if gs.is_chance_node() {
                let a = agent.step(&gs);
                gs.apply_action(a);
            } else {
                let legal_move_count = actions!(gs).len();
                end_states *= legal_move_count;
                total_states += end_states;
                children[round] += legal_move_count as f64;
                round += 1;
                let a = agent.step(&gs);
                gs.apply_action(a);
            }
        }
        total_end_states += end_states;
        _total_rounds += round;
    }

    println!("average post deal end states: {}", total_end_states / runs);
    println!("average post deal states: {}", total_states / runs);
    // println!("rounds: {}", total_rounds / runs);
    // let mut sum = 1.0;
    // for (i, c) in children.iter().enumerate() {
    //     println!(
    //         "round {} has {} children, {} peers",
    //         i,
    //         c / runs as f64,
    //         sum
    //     );
    //     sum *= (c / runs as f64).max(1.0);
    // }

    // traverse gametress
    let mut gs = Euchre::new_state();
    // let mut s = KuhnPoker::new_state();

    // TODO: A seed of 0 here seems to break things. Why?
    let mut rng: StdRng = SeedableRng::seed_from_u64(0);
    while gs.is_chance_node() {
        let a = *actions!(gs).choose(&mut rng).unwrap();
        gs.apply_action(a);
    }

    println!("total storable nodes: {}", traverse_game_tree(gs, 0));
}

fn traverse_game_tree<T: GameState>(gs: T, depth: usize) -> usize {
    if gs.is_terminal() {
        return 0; // don't need to store leaf node
    }

    let mut count = 1;
    for a in actions!(gs) {
        if depth <= 2 {
            println!("depth: {}, nodes: {}", depth, count)
        }

        let mut new_s = gs.clone();
        new_s.apply_action(a);

        // don't need to store if only 1 action
        while actions!(new_s).len() == 1 {
            new_s.apply_action(actions!(new_s)[0])
        }

        count += traverse_game_tree(new_s, depth + 1);
    }

    count
}
