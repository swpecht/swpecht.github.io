use criterion::{black_box, criterion_group, criterion_main, Criterion};
use liars_poker_bot::{euchre::Euchre, game::GameState};
use rand::{seq::SliceRandom, thread_rng};

use liars_poker_bot::{cfragent::CFRAgent, database::Storage, kuhn_poker::KuhnPoker};

fn train_cfr_kp() {
    let game = KuhnPoker::game();
    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    CFRAgent::new(game, 42, 100, Storage::Memory);
}

/// Attempts to mimic the call structure of CFR without actually doing it
fn traverse_game_tree(n: usize) {
    let game = Euchre::game();
    let mut s = (game.new)();

    while s.is_chance_node() {
        let actions = s.legal_actions();
        let a = *actions.choose(&mut thread_rng()).unwrap();
        s.apply_action(a);
    }

    let mut work = Vec::new();
    work.push((s.information_state(s.cur_player()), s));

    let mut nodes_processed = 0;

    while nodes_processed < n {
        nodes_processed += 1;

        let (_, s) = work.pop().unwrap();
        let actions = s.legal_actions();
        for a in actions {
            let mut new_s = s.clone();
            new_s.apply_action(a);
            let istate = new_s.information_state(new_s.cur_player());
            work.push((istate, new_s));
        }
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("traverse euchre game tree", |b| {
        b.iter(|| traverse_game_tree(black_box(10000)))
    });

    c.bench_function("cfr kuhn poker 100", |b| b.iter(|| train_cfr_kp()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);