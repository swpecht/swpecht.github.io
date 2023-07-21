use clap::Args;
use indicatif::ProgressBar;
use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, Seedable},
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
use log::info;
use rand::{seq::SliceRandom, thread_rng, SeedableRng};

use super::{benchmark::get_rng, pass_on_bower::PassOnBowerIterator};

#[derive(Args, Clone, Debug)]
pub struct PassOnBowerCFRArgs {
    training_iterations: usize,
    #[clap(short, long, default_value_t = 200)]
    scoring_iterations: usize,
    #[clap(long, default_value_t = 1000)]
    checkpoint_freq: usize,
    #[clap(long, default_value_t = 10000)]
    scoring_freq: usize,
    #[clap(long, default_value = "infostates")]
    weight_file: String,
}

pub fn run_pass_on_bower_cfr(args: PassOnBowerCFRArgs) {
    info!("starting new run of pass on bower cfr. args {:?}", args);

    let generator = generate_jack_of_spades_deal;
    let pb = ProgressBar::new(args.training_iterations as u64);
    let mut alg = CFRES::new_euchre_bidding(generator, get_rng());

    let infostate_path = args.weight_file.as_str();
    let loaded_states = alg.load(infostate_path);
    info!(
        "loaded {} info states from {}",
        loaded_states, infostate_path
    );

    let worlds = (0..args.scoring_iterations)
        .map(|_| generate_jack_of_spades_deal())
        .collect_vec();
    let mut baseline = PIMCTSBot::new(50, OpenHandSolver::new_euchre(), get_rng());
    info!("calculating baseline performance...");
    let baseline_score = score_vs_defender(&mut baseline, 1, worlds.clone());

    // print_scored_istates(&mut alg);

    for i in 0..args.training_iterations {
        alg.train(1);
        pb.inc(1);
        if i % args.checkpoint_freq == 0 && i > 0 {
            alg.save(infostate_path);
        }

        if i % args.scoring_freq == 0 {
            log_score(&mut alg, i, worlds.clone(), baseline_score);
        }
    }
    pb.finish_and_clear();
    alg.save(infostate_path);
    println!("num info states: {}", alg.num_info_states());

    log_score(&mut alg, args.training_iterations, worlds, baseline_score);
}

fn log_score(
    alg: &mut CFRES<EuchreGameState>,
    iteration: usize,
    worlds: Vec<EuchreGameState>,
    baseline_score: f64,
) {
    let score = score_vs_defender(alg, 1, worlds);
    info!(
        "iteration:\t{}\tnodes touched:\t{}\tinfo_states:\t{}\tscore:\t{}\tbaseline:\t{}",
        iteration,
        read_counter("cfr.cfres.nodes_touched"),
        alg.num_info_states(),
        score,
        baseline_score,
    );
}

fn score_vs_defender<A: Agent<EuchreGameState> + Seedable>(
    target: &mut A,
    target_team: usize,
    worlds: Vec<EuchreGameState>,
) -> f64 {
    let mut running_score = 0.0;
    for (i, mut w) in worlds.clone().into_iter().enumerate() {
        // have a consistent seed for the defender each game
        let mut defender = PIMCTSBot::new(
            50,
            OpenHandSolver::new_euchre(),
            SeedableRng::seed_from_u64(i as u64),
        );

        // magic number offset so the games are the same as the defender
        target.set_seed(i as u64 + 42);

        while !w.is_terminal() {
            let cur_player = w.cur_player();
            let a = match cur_player % 2 == target_team {
                true => target.step(&w),
                false => defender.step(&w),
            };
            w.apply_action(a);
        }

        running_score += w.evaluate(target_team);
    }
    running_score / worlds.len() as f64
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
