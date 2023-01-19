use criterion::{criterion_group, criterion_main, Criterion};
use liars_poker_bot::{cfragent::CFRAgent, database::Storage, kuhn_poker::KuhnPoker};

fn train_cfr_kp() {
    let game = KuhnPoker::game();
    // Verify the nash equilibrium is reached. From https://en.wikipedia.org/wiki/Kuhn_poker
    CFRAgent::new(game, 42, 100, Storage::Memory);
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("cfr kp 100", |b| b.iter(|| train_cfr_kp()));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
