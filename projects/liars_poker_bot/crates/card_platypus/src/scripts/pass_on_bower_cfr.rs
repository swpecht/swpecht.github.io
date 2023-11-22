use std::{collections::HashMap, default, fs, ops::Deref, path::Path};

use card_platypus::{
    agents::{Agent, Seedable},
    algorithms::cfres::{self, CFRES},
    algorithms::{open_hand_solver::OpenHandSolver, pimcts::PIMCTSBot},
};
use clap::{Args, ValueEnum};
use games::{
    actions,
    gamestates::euchre::{
        actions::{Card, EAction},
        ismorphic::LossyEuchreNormalizer,
        util::generate_face_up_deals,
        Euchre, EuchreGameState,
    },
    resample::ResampleFromInfoState,
    GameState,
};
use indicatif::ProgressBar;
use itertools::Itertools;
use log::info;
use rand::{seq::SliceRandom, thread_rng, Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use super::benchmark::get_rng;

#[derive(ValueEnum, Copy, Clone, Debug, Deserialize, Default)]
enum DealType {
    JackOfSpadesOnly,
    #[default]
    All,
}

#[derive(ValueEnum, Copy, Clone, Debug, Deserialize, Default)]
enum Normalizer {
    #[default]
    Lossless,
    Lossy,
}

#[derive(Args, Clone, Debug, Deserialize)]
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
    #[serde(default)]
    deal_type: DealType,
    #[clap(long, default_value_t = false)]
    #[serde(default)]
    no_linear_cfr: bool,
    #[clap(long, default_value_t = false)]
    #[serde(default)]
    single_thread: bool,
    #[clap(long, default_value_t = 0)]
    max_cards_played: usize,
    #[clap(long, value_enum, default_value_t=Normalizer::Lossless)]
    #[serde(default)]
    normalizer: Normalizer,
}

pub fn run_pass_on_bower_cfr(args: PassOnBowerCFRArgs) {
    if !args.no_linear_cfr {
        cfres::feature::enable(cfres::feature::LinearCFR);
    } else {
        cfres::feature::disable(cfres::feature::LinearCFR);
    }

    if args.single_thread {
        cfres::feature::enable(cfres::feature::SingleThread);
    } else {
        cfres::feature::disable(cfres::feature::SingleThread);
    }

    match args.deal_type {
        DealType::JackOfSpadesOnly => {
            let n = args.training_iterations;
            train_cfr_shot(args, n, || generate_face_up_deals(Card::JS))
        }
        DealType::All => all_deal_cfr(args),
    }
}

pub fn all_deal_cfr(args: PassOnBowerCFRArgs) {
    info!("starting new run of cfr. args {:?}", args);
    let iterations_per_card = args.training_iterations / 6;

    train_cfr_shot(args.clone(), iterations_per_card, || {
        generate_face_up_deals(Card::NS)
    });
    train_cfr_shot(args.clone(), iterations_per_card, || {
        generate_face_up_deals(Card::TS)
    });
    train_cfr_shot(args.clone(), iterations_per_card, || {
        generate_face_up_deals(Card::JS)
    });
    train_cfr_shot(args.clone(), iterations_per_card, || {
        generate_face_up_deals(Card::QS)
    });
    train_cfr_shot(args.clone(), iterations_per_card, || {
        generate_face_up_deals(Card::KS)
    });
    train_cfr_shot(args, iterations_per_card, || {
        generate_face_up_deals(Card::AS)
    });
}

pub fn train_cfr_shot(
    args: PassOnBowerCFRArgs,
    training_iterations: usize,
    generator: fn() -> EuchreGameState,
) {
    let pb = ProgressBar::new(training_iterations as u64);
    let mut alg = match args.normalizer {
        Normalizer::Lossless => CFRES::new_euchre(generator, get_rng(), args.max_cards_played),
        Normalizer::Lossy => CFRES::new_with_normalizer(
            generator,
            get_rng(),
            args.max_cards_played,
            Box::<LossyEuchreNormalizer>::default(),
        ),
    };

    let infostate_path = args.weight_file.as_str();
    let loaded_states = alg.load(Path::new(infostate_path));
    info!(
        "loaded {} info states from {}",
        loaded_states, infostate_path
    );

    let worlds = (0..args.scoring_iterations)
        .map(|_| {
            let mut gs = Euchre::new_state();
            while gs.is_chance_node() {
                let actions = actions!(gs);
                let a = actions.choose(&mut thread_rng()).unwrap();
                gs.apply_action(*a);
            }
            gs
        })
        .collect_vec();
    let mut baseline = PIMCTSBot::new(50, OpenHandSolver::new_euchre(), get_rng());
    // let mut baseline = CFRES::new_euchre_bidding(generator, get_rng(), 0);
    // baseline.load("/var/lib/card_platypus/infostate.baseline");

    info!("calculating baseline performance...");
    let baseline_score = score_vs_defender(&mut baseline, 1, worlds.clone());
    info!("found baseline performance of: {}", baseline_score);

    // print_scored_istates(&mut alg);

    const TRAINING_PER_ITERATION: usize = 100;
    for i in 0..training_iterations / TRAINING_PER_ITERATION {
        alg.train(TRAINING_PER_ITERATION);
        pb.inc(TRAINING_PER_ITERATION as u64);
        if (i * TRAINING_PER_ITERATION) % args.checkpoint_freq == 0 && i > 0 {
            alg.save().unwrap();
        }

        if (i * TRAINING_PER_ITERATION) % args.scoring_freq == 0 {
            log_score(&mut alg, worlds.clone(), baseline_score);
            // reset to a random seed for future training evaluation
            alg.set_seed(get_rng().gen());
        }
    }
    pb.finish_and_clear();
    alg.save().unwrap();
    println!("num info states: {}", alg.num_info_states());

    log_score(&mut alg, worlds, baseline_score);
}

fn log_score(alg: &mut CFRES<EuchreGameState>, worlds: Vec<EuchreGameState>, baseline_score: f64) {
    let score = score_vs_defender(alg, 1, worlds);
    info!(
        "iteration:\t{}\tnodes touched:\t{}\tinfo_states:\t{}\tscore:\t{}\tbaseline:\t{}",
        alg.iterations(),
        cfres::nodes_touched::read(),
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

    // let mut defender = CFRES::new_euchre_bidding(Euchre::new_state, get_rng(), 0);
    // defender.load("/var/lib/card_platypus/infostate.baseline");

    for (i, mut w) in worlds.clone().into_iter().enumerate() {
        // have a consistent seed for the defender each game
        let mut defender = PIMCTSBot::new(
            50,
            OpenHandSolver::new_euchre(),
            SeedableRng::seed_from_u64(i as u64),
        );

        // magic number offset so the games are the same as the defender
        defender.set_seed(i as u64);
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

#[derive(Serialize)]
struct JSONRow {
    infostate: String,
    hand: Vec<String>,
    policy: HashMap<String, f64>,
}

pub fn parse_weights(infostate_path: &str) {
    let generator = || generate_face_up_deals(Card::JS);
    let mut alg = CFRES::new_euchre(generator, get_rng(), 0);

    let loaded_states = alg.load(Path::new(infostate_path));
    println!(
        "loaded {} info states from {}",
        loaded_states, infostate_path
    );

    let infostates = alg.get_infostates();
    let mut json_infostates = Vec::new();

    let pb = ProgressBar::new(infostates.len() as u64);
    for entry in infostates.deref() {
        let k = entry.key();
        let v = entry.value();
        // filter for the istate keys that end in the right actions
        // if k[k.len() - 1] != EAction::DiscardMarker.into() {
        let istate = k
            .iter()
            .map(|&x| EAction::from(x).to_string())
            .collect_vec();

        let policy_sum: f64 = v
            .avg_strategy()
            .to_vec()
            .iter()
            .map(|(_, v)| *v as f64)
            .sum();
        let mut policy = HashMap::new();

        for (a, w) in v.avg_strategy() {
            // we can undo the normalization here
            let action = EAction::from(a.get()).to_string();
            policy.insert(action, w as f64 / policy_sum);
        }

        json_infostates.push(JSONRow {
            infostate: istate.join(""),
            hand: istate[..5].to_vec(),
            policy,
        });
        // }
        pb.inc(1);
    }
    pb.finish_and_clear();

    // Save a csv file
    let json_data = serde_json::to_string(&json_infostates).unwrap();
    let mut json_path = infostate_path.to_string();
    json_path.push_str(".json");
    fs::write(json_path.clone(), json_data).unwrap();
    println!("json weights written to: {json_path}");
}

pub fn analyze_istate(num_games: usize) {
    let istate = EuchreGameState::from("9sTsQsKsAs|9cTcKcAcTd|JdQdKdAd9h|JcQcJhAh9d|Js");
    let mut rng = get_rng();
    let mut agent = CFRES::new_euchre(Euchre::new_state, rng.clone(), 0);
    let loaded = agent.load(Path::new("/var/lib/card_platypus/infostate.baseline"));
    info!("loaded {}", loaded);

    let mut pass_on_bower_games = Vec::new();
    for _ in 0..num_games {
        let mut gs = istate.resample_from_istate(3, &mut rng);

        let mut always_pass = true;
        for _ in 0..3 {
            let a = agent.step(&gs);
            let ea = EAction::from(a);
            match ea {
                EAction::Pass => {}
                _ => {
                    always_pass = false;
                    break;
                }
            };

            gs.apply_action(EAction::Pass.into());
        }

        if always_pass {
            pass_on_bower_games.push(gs);
        }
    }

    println!("{}", pass_on_bower_games.len());
    println!("{}", pass_on_bower_games[0]);

    // Get outcome distribution for pass games
    let pass_counts = outcome_distribution(
        pass_on_bower_games
            .clone()
            .into_iter()
            .map(|mut gs| {
                gs.apply_action(EAction::Pass.into());
                gs
            })
            .collect_vec(),
        &mut agent,
    );

    println!(
        "pass counts:\n{}",
        serde_json::to_string_pretty(&pass_counts).unwrap()
    );

    let take_counts = outcome_distribution(
        pass_on_bower_games
            .clone()
            .into_iter()
            .map(|mut gs| {
                gs.apply_action(EAction::Pickup.into());
                gs
            })
            .collect_vec(),
        &mut agent,
    );

    println!(
        "take counts:\n{}",
        serde_json::to_string_pretty(&take_counts).unwrap()
    );
}

fn outcome_distribution(
    games: Vec<EuchreGameState>,
    agent: &mut CFRES<EuchreGameState>,
) -> HashMap<String, HashMap<i8, usize>> {
    let mut counts = HashMap::new();
    let pb = ProgressBar::new(games.len() as u64);
    for mut gs in games {
        loop {
            let a = agent.step(&gs);
            let ea = EAction::from(a);
            gs.apply_action(a);
            match ea {
                EAction::Pass => {}
                _ => {
                    break;
                }
            };
        }

        let trump_call = gs.istate_string(3);

        while !gs.is_terminal() {
            let a = agent.step(&gs);
            gs.apply_action(a);
        }

        let score = gs.evaluate(3) as i8;

        let score_distribution: &mut HashMap<i8, usize> = counts.entry(trump_call).or_default();
        let c = score_distribution.entry(score).or_default();
        *c += 1;
        pb.inc(1);
    }

    pb.finish_and_clear();
    counts
}
