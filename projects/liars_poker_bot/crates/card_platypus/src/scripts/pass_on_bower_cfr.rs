use std::{collections::HashMap, fs};

use card_platypus::{
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
};
use clap::{Args, ValueEnum};
use indicatif::ProgressBar;
use itertools::Itertools;
use log::info;
use rand::{seq::SliceRandom, thread_rng, Rng, SeedableRng};

use super::benchmark::get_rng;

#[derive(ValueEnum, Copy, Clone, Debug)]
enum DealType {
    JackOfSpadesOnly,
    All,
}

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
    #[clap(long, value_enum, default_value_t=DealType::All)]
    deal_type: DealType,
}

pub fn run_pass_on_bower_cfr(args: PassOnBowerCFRArgs) {
    match args.deal_type {
        DealType::JackOfSpadesOnly => train_cfr(args, generate_jack_of_spades_deal),
        DealType::All => train_cfr(args, Euchre::new_state),
    }
}

pub fn train_cfr(args: PassOnBowerCFRArgs, generator: fn() -> EuchreGameState) {
    info!("starting new run of pass on bower cfr. args {:?}", args);

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
    info!("found baseline performance of: {}", baseline_score);

    // print_scored_istates(&mut alg);

    const TRAINING_PER_ITERATION: usize = 100;
    for i in 0..args.training_iterations / TRAINING_PER_ITERATION {
        alg.train(TRAINING_PER_ITERATION);
        pb.inc(TRAINING_PER_ITERATION as u64);
        if (i * TRAINING_PER_ITERATION) % args.checkpoint_freq == 0 && i > 0 {
            alg.save(infostate_path);
        }

        if (i * TRAINING_PER_ITERATION) % args.scoring_freq == 0 {
            log_score(
                &mut alg,
                i * TRAINING_PER_ITERATION,
                worlds.clone(),
                baseline_score,
            );
            // reset to a random seed for future training evaluation
            alg.set_seed(get_rng().gen());
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

pub fn parse_weights(infostate_path: &str) {
    let generator = generate_jack_of_spades_deal;
    let mut alg = CFRES::new_euchre_bidding(generator, get_rng());

    let loaded_states = alg.load(infostate_path);
    println!(
        "loaded {} info states from {}",
        loaded_states, infostate_path
    );

    let infostates = alg.get_infostates();

    for (k, v) in infostates.clone() {
        // filter for the istate keys that end in the right actions
        if k[k.len() - 3..]
            .iter()
            .all(|&x| EAction::from(x) == EAction::Pass)
            && EAction::from(k[k.len() - 4]) != EAction::Pass
        {
            let istate = k[..k.len() - 4]
                .iter()
                .map(|&x| EAction::from(x).to_string())
                .collect_vec()
                .join("\t");

            let policy_sum: f64 = v.avg_strategy().to_vec().iter().map(|(_, v)| *v).sum();
            let take_prob = v.avg_strategy()[EAction::Pickup.into()] / policy_sum;

            info!("\t{}\t{}\t{}", istate, take_prob, v.update_count());
        }
    }
    println!("pass on bower take probs written to log file");

    // convert to a key string
    let mut json_infostates = HashMap::with_capacity(infostates.len());
    for (k, v) in infostates {
        let istate_string = k
            .iter()
            .map(|&x| EAction::from(x).to_string())
            .collect_vec()
            .join("");
        json_infostates.insert(istate_string, v);
    }

    // Save a csv file
    let json_data = serde_json::to_string(&json_infostates).unwrap();
    let mut json_path = infostate_path.to_string();
    json_path.push_str(".json");
    fs::write(json_path.clone(), json_data).unwrap();
    println!("json weights written to: {json_path}");
}