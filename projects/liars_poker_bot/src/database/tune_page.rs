use log::trace;
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

use crate::{
    game::euchre::{Euchre, EuchreGameState},
    game::GameState,
};

struct Counter {
    cur_count: usize,
    total_count: usize,
    num_trees: usize,
}

impl Counter {
    fn new() -> Self {
        Self {
            cur_count: 0,
            total_count: 0,
            num_trees: 0,
        }
    }
}

/// Finds the right size for Page breaks by calculating how many nodes are children of a node `n` actions into the game
pub fn tune_page_size() {
    println!("tuning page size for euchre...");

    let min_depth = 5;
    let max_depth = 999;

    let mut rng: StdRng = SeedableRng::seed_from_u64(42);

    let mut c = Counter::new();

    for _ in 0..10 {
        let mut gs = Euchre::new_state();
        while gs.is_chance_node() {
            let a = *gs.legal_actions().choose(&mut rng).unwrap();
            gs.apply_action(a);
        }

        count_tree(gs, min_depth, max_depth, &mut c);

        println!("total children: {}", c.total_count);
        println!("num_trees: {}", c.num_trees);
        println!(
            "children per tree: {}",
            c.total_count as f64 / c.num_trees as f64
        );
    }
}

fn count_tree(gs: EuchreGameState, min_depth: usize, max_depth: usize, c: &mut Counter) {
    let mut q = Vec::new();
    q.push((gs, 0));

    while let Some((gs, depth)) = q.pop() {
        if gs.is_terminal() || depth > max_depth {
            continue;
        }

        if depth == min_depth {
            trace!(
                "evaluating {} at depth {}, was count was {}",
                gs.istate_string(0),
                depth,
                c.cur_count
            );
            c.total_count += c.cur_count;
            c.num_trees += 1;
            c.cur_count = 0; // reset the count at the min depth
        }

        if depth >= min_depth && depth <= max_depth {
            c.cur_count += 1;
        }

        for a in gs.legal_actions() {
            let mut cur_depth = depth;
            let mut new_s = gs;
            new_s.apply_action(a);

            // don't need to store if only 1 action
            while new_s.legal_actions().len() == 1 {
                new_s.apply_action(new_s.legal_actions()[0]);
                cur_depth += 1;
            }

            if depth + 1 <= max_depth {
                q.push((new_s, cur_depth + 1));
            }
        }
    }

    c.total_count += c.cur_count;
    c.cur_count = 0;
}
