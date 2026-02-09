use std::time::Duration;

use card_platypus::algorithms::{
    ismcts::Evaluator, open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot,
};
use criterion::{criterion_group, criterion_main, Criterion};
use games::{
    actions,
    gamestates::euchre::{Euchre, EuchreGameState},
    GameState,
};
use rand::{rngs::StdRng, seq::IndexedRandom, SeedableRng};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut evaluator = PIMCTSBot::new(
        50,
        OpenHandSolver::new_euchre(),
        SeedableRng::seed_from_u64(100),
    );

    let mut group = c.benchmark_group("open-hand");
    let mut rng: StdRng = SeedableRng::seed_from_u64(101);
    group.throughput(criterion::Throughput::Elements(1));
    group.sample_size(2000);
    group.measurement_time(Duration::new(35, 0));
    group.bench_function("open hand evaluator 50", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });
    group.finish();
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
