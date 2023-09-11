use std::time::Duration;

use card_platypus::{
    actions,
    algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
    game::{
        euchre::{Euchre, EuchreGameState},
        GameState,
    },
};
use criterion::{criterion_group, criterion_main, Criterion};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut evaluator = PIMCTSBot::new(
        50,
        OpenHandSolver::new_euchre(),
        SeedableRng::seed_from_u64(100),
    );

    let mut array = [1; 7];
    let mut v = 1;

    c.bench_function("shift rotate", |b| b.iter(|| rotate_array(&mut array)));

    c.bench_function("shift bitshift", |b| b.iter(|| bit_shift(&mut v)));

    let mut group = c.benchmark_group("open-hand");
    let mut rng: StdRng = SeedableRng::seed_from_u64(101);
    group.throughput(criterion::Throughput::Elements(1));
    group.sample_size(2000);
    group.measurement_time(Duration::new(35, 0));
    group.bench_function("open hand evaluator 50", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });
    group.finish();

    let mut group = c.benchmark_group("agents");
    group.sample_size(10);
}

fn rotate_array(array: &mut [u8]) {
    array[1..].rotate_left(1);
}

fn bit_shift(v: &mut u32) {
    let x = *v & 0b1111;
    *v >>= 4;
    *v &= !(0b1111);
    *v |= x;
}

fn evaluate_games(
    evaluator: &mut PIMCTSBot<EuchreGameState, OpenHandSolver<EuchreGameState>>,
    rng: &mut StdRng,
) {
    let gs = &get_game(rng);
    evaluator.evaluate_player(gs, 3);
}

fn get_game(rng: &mut StdRng) -> EuchreGameState {
    let mut gs = Euchre::new_state();
    while gs.is_chance_node() {
        let a = *actions!(gs).choose(rng).unwrap();
        gs.apply_action(a)
    }

    gs
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
