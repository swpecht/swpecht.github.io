use criterion::{black_box, criterion_group, criterion_main, Criterion};
use liars_poker_bot::{
    actions,
    cfragent::CFRAlgorithm,
    database::memory_node_store::MemoryNodeStore,
    game::euchre::{Euchre, EuchreGameState},
    game::GameState,
};
use rand::{seq::SliceRandom, thread_rng};

use liars_poker_bot::{cfragent::CFRAgent, game::kuhn_poker::KuhnPoker};

fn train_cfr_kp() {
    let game = KuhnPoker::game();
    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    let mut agent = CFRAgent::new(game, 42, MemoryNodeStore::default(), CFRAlgorithm::CFRCS);
    agent.train(100);
}

/// Attempts to mimic the call structure of CFR without actually doing it
fn traverse_game_tree(n: usize) {
    let game = Euchre::game();
    let mut gs = (game.new)();

    while gs.is_chance_node() {
        let actions = actions!(gs);
        let a = *actions.choose(&mut thread_rng()).unwrap();
        gs.apply_action(a);
    }

    let mut work = Vec::new();
    work.push((gs.istate_key(gs.cur_player()), gs));

    let mut pool: Vec<EuchreGameState> = Vec::new();

    let mut nodes_processed = 0;

    while nodes_processed < n {
        nodes_processed += 1;

        let (_, gs) = work.pop().unwrap();
        let actions = actions!(gs);
        for a in actions {
            let mut new_s = new_gs(&gs, &mut pool);

            new_s.apply_action(a);
            let istate = new_s.istate_key(new_s.cur_player());
            work.push((istate, new_s));
        }

        pool.push(gs);
    }
}

fn new_gs(g: &EuchreGameState, pool: &mut Vec<EuchreGameState>) -> EuchreGameState {
    if let Some(mut new_s) = pool.pop() {
        new_s = *g;
        new_s
    } else {
        *g
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("traverse euchre game tree", |b| {
        b.iter(|| traverse_game_tree(black_box(10000)))
    });

    c.bench_function("cfr kuhn poker 100", |b| b.iter(train_cfr_kp));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
