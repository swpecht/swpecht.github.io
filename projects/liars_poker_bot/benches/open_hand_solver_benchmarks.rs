use criterion::{criterion_group, criterion_main, Criterion};
use liars_poker_bot::{
    actions,
    algorithms::open_hand_solver::OpenHandSolver,
    game::{
        euchre::{Euchre, EuchreGameState},
        GameState,
    },
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = OpenHandSolver::new(100, SeedableRng::seed_from_u64(100));

    c.bench_function("open hand evaluator 100", |b| {
        b.iter(|| evaluate_games(&mut evaluator, &mut rng))
    });
}

fn evaluate_games(evaluator: &mut OpenHandSolver<EuchreGameState>, rng: &mut StdRng) {
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
