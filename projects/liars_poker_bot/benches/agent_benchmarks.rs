use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use liars_poker_bot::{
    actions,
    agents::PolicyAgent,
    algorithms::{
        alphamu::AlphaMuBot, ismcts::Evaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot,
    },
    game::{
        euchre::{Euchre, EuchreGameState},
        run_game, GameState,
    },
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);
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
    group.sample_size(1000);
    group.measurement_time(Duration::new(35, 0));
    group.bench_function("open hand evaluator 50", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });
    group.finish();

    let mut group = c.benchmark_group("agents");
    group.sample_size(10);

    let rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = PolicyAgent::new(
        AlphaMuBot::new(OpenHandSolver::new_euchre(), 10, 5, rng.clone()),
        rng,
    );
    let mut rng: StdRng = SeedableRng::seed_from_u64(45);
    group.bench_function("alpha mu 10 worlds, m=5", |b| {
        b.iter(|| alpha_mu_benchmark(&mut evaluator, &mut rng))
    });

    let rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = AlphaMuBot::new(OpenHandSolver::new(), 20, 5, rng);
    let mut rng: StdRng = SeedableRng::seed_from_u64(45);
    group.bench_function("alpha mu 20 worlds, m=5", |b| {
        b.iter(|| alpha_mu_eval_benchmark(&mut evaluator, &mut rng))
    });

    let rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = AlphaMuBot::new(OpenHandSolver::new(), 30, 3, rng);
    let mut rng: StdRng = SeedableRng::seed_from_u64(45);
    group.bench_function("alpha mu 30 worlds, m=3", |b| {
        b.iter(|| alpha_mu_eval_benchmark(&mut evaluator, &mut rng))
    });
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

fn alpha_mu_benchmark(
    evaluator: &mut PolicyAgent<AlphaMuBot<EuchreGameState, OpenHandSolver<EuchreGameState>>>,
    rng: &mut StdRng,
) {
    let mut gs = get_game(rng);
    run_game(&mut gs, evaluator, &mut None, rng);
}

fn alpha_mu_eval_benchmark(
    evaluator: &mut AlphaMuBot<EuchreGameState, OpenHandSolver<EuchreGameState>>,
    rng: &mut StdRng,
) {
    let gs = get_game(rng);
    evaluator.run_search(&gs, gs.cur_player());
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
