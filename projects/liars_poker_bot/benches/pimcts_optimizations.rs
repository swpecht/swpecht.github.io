use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use liars_poker_bot::{
    actions,
    algorithms::{
        ismcts::Evaluator,
        open_hand_solver::{OpenHandSolver, Optimizations, DEFAULT_MAX_TT_DEPTH},
        pimcts::PIMCTSBot,
    },
    game::{
        euchre::{Euchre, EuchreGameState},
        Action, GameState,
    },
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("pimcts-optimizations");
    let mut rng: StdRng = SeedableRng::seed_from_u64(101);
    group.throughput(criterion::Throughput::Elements(1));
    group.sample_size(100);
    group.measurement_time(Duration::new(35, 0));

    let mut evaluator = get_evaluator(Optimizations {
        use_transposition_table: false,
        isometric_transposition: false,
        max_depth_for_tt: 255,
        action_processor: |_: &EuchreGameState, _: &mut Vec<Action>| {},
    });
    group.bench_function("euchre solver: no optimizations", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });

    group.sample_size(200);
    let mut evaluator = get_evaluator(Optimizations {
        use_transposition_table: true,
        isometric_transposition: false,
        max_depth_for_tt: 255,
        action_processor: |_: &EuchreGameState, _: &mut Vec<Action>| {},
    });
    group.bench_function("euchre solver: add transposition table", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });

    group.sample_size(2000);
    let mut evaluator = get_evaluator(Optimizations {
        use_transposition_table: true,
        isometric_transposition: true,
        max_depth_for_tt: 255,
        action_processor: |_: &EuchreGameState, _: &mut Vec<Action>| {},
    });
    group.bench_function("euchre solver: add isometric representation", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });

    group.sample_size(2000);
    let mut evaluator = get_evaluator(Optimizations {
        use_transposition_table: true,
        isometric_transposition: true,
        max_depth_for_tt: DEFAULT_MAX_TT_DEPTH,
        action_processor: |_: &EuchreGameState, _: &mut Vec<Action>| {},
    });
    group.bench_function("euchre solver: limit tt depth", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });

    let mut evaluator = get_evaluator(Optimizations::new_euchre());
    group.bench_function("euchre solver: all optimizations", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });
    group.finish();
}

fn get_evaluator(
    optimizations: Optimizations<EuchreGameState>,
) -> PIMCTSBot<EuchreGameState, OpenHandSolver<EuchreGameState>> {
    let solver = OpenHandSolver::new_euchre(optimizations);
    PIMCTSBot::new(50, solver, SeedableRng::seed_from_u64(100))
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
