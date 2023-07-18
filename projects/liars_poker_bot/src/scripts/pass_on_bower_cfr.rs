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
};
use rand::{seq::SliceRandom, thread_rng};

use super::benchmark::get_rng;

pub fn run_pass_on_bower_cfr() {
    let generator = generate_jack_of_spades_deal;
    let num_iterations = 5_000;
    let pb = ProgressBar::new(num_iterations as u64);
    let mut alg = CFRES::new_euchre_bidding(generator, get_rng());
    for _ in 0..num_iterations {
        alg.train(1);
        pb.inc(1);
    }
    pb.finish_and_clear();
    println!("num info states: {}", alg.num_info_states());

    let mut opponent = PolicyAgent::new(
        PIMCTSBot::new(50, OpenHandSolver::new_euchre(), get_rng()),
        get_rng(),
    );
    let mut cfr_agent = PolicyAgent::new(alg, get_rng());

    let worlds = (0..500)
        .map(|_| generate_jack_of_spades_deal())
        .collect_vec();
    let mut running_score = 0.0;
    for mut w in worlds.clone() {
        while !w.is_terminal() {
            let cur_player = w.cur_player();
            let a = match cur_player % 2 == 0 {
                true => opponent.step(&w),
                false => cfr_agent.step(&w),
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
