use indicatif::ProgressBar;
use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
    cfragent::cfres::CFRES,
    game::{
        euchre::{
            actions::{Card, EAction},
            Euchre, EuchreGameState,
        },
        GameState,
    },
    metrics::read_counter,
    policy::Policy,
};
use rand::{seq::SliceRandom, thread_rng};

use super::{benchmark::get_rng, pass_on_bower::PassOnBowerIterator};

pub fn run_pass_on_bower_cfr(training_iterations: usize) {
    let generator = generate_jack_of_spades_deal;
    let pb = ProgressBar::new(training_iterations as u64);
    let mut alg = CFRES::new_euchre_bidding(generator, get_rng());

    let infostate_path = "infostates";
    alg.load(infostate_path);

    print_scored_istates(&mut alg);

    for i in 0..training_iterations {
        alg.train(1);
        pb.inc(1);
        if i % 1000 == 0 && i > 0 {
            alg.save(infostate_path);
            println!("nodes touched: {}", read_counter("cfr.cfres.nodes_touched"))
        }
    }
    pb.finish_and_clear();
    alg.save(infostate_path);
    println!("num info states: {}", alg.num_info_states());

    let mut opponent = PolicyAgent::new(
        PIMCTSBot::new(50, OpenHandSolver::new_euchre(), get_rng()),
        get_rng(),
    );
    let mut cfr_agent = PolicyAgent::new(alg, get_rng());

    let worlds = (0..100)
        .map(|_| generate_jack_of_spades_deal())
        .collect_vec();
    let mut running_score = 0.0;
    for mut w in worlds.clone() {
        while !w.is_terminal() {
            let cur_player = w.cur_player();
            let a = match cur_player == 3 || cur_player == 1 {
                true => cfr_agent.step(&w),
                false => opponent.step(&w),
            };
            w.apply_action(a);
        }

        running_score += w.evaluate(3);
    }

    println!(
        "cfr player 3 score: {}",
        running_score / worlds.len() as f64
    );

    let mut running_score = 0.0;
    for mut w in worlds.clone() {
        while !w.is_terminal() {
            let a = opponent.step(&w);
            w.apply_action(a);
        }

        running_score += w.evaluate(3);
    }

    println!(
        "pimcts player 3 score: {}",
        running_score / worlds.len() as f64
    );
}

fn print_scored_istates(alg: &mut CFRES<EuchreGameState>) {
    let games = PassOnBowerIterator::new();
    for gs in games {
        let policy = alg.action_probabilities(&gs);
        if !policy.to_vec().iter().all(|(_, b)| *b == 0.5) {
            println!(
                "{}: {:?}",
                gs,
                policy
                    .to_vec()
                    .into_iter()
                    .map(|(a, b)| (EAction::from(a), b))
                    .collect_vec()
            );
        }
    }
}

/// Generator for games where the jack of spades is face up
pub fn generate_jack_of_spades_deal() -> EuchreGameState {
    let mut gs = Euchre::new_state();
    let mut actions = Vec::new();
    for _ in 0..20 {
        gs.legal_actions(&mut actions);
        actions.retain(|&a| EAction::from(a).card() != Card::JS);
        let a = actions
            .choose(&mut thread_rng())
            .expect("error dealing cards");
        gs.apply_action(*a);
        actions.clear();
    }

    gs.apply_action(EAction::DealFaceUp { c: Card::JS }.into());

    gs
}
