use criterion::{black_box, criterion_group, criterion_main, Criterion};
use liars_poker_bot::{
    actions,
    algorithms::{ismcts::Evaluator, open_hand_solver::OpenHandSolver},
    game::{euchre::Euchre, GameState},
    policy::Policy,
};
use rand::{rngs::StdRng, seq::SliceRandom, SeedableRng};

pub fn criterion_benchmark(c: &mut Criterion) {
    let mut rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = OpenHandSolver::new(100, rng.clone());

    let mut gs = Euchre::new_state();
    while gs.is_chance_node() {
        let a = *actions!(gs).choose(&mut rng).unwrap();
        gs.apply_action(a)
    }

    c.bench_function("open hand evaluator 100", |b| {
        b.iter(|| evaluator.action_probabilities(black_box(&gs)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
