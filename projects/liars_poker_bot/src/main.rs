use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::mem;

use clap::{command, Parser, Subcommand, ValueEnum};

use itertools::Itertools;
use liars_poker_bot::actions;
use liars_poker_bot::agents::{Agent, PlayerAgent, PolicyAgent};

use liars_poker_bot::algorithms::exploitability::{self};

use liars_poker_bot::algorithms::ismcts::{Evaluator, ResampleFromInfoState};
use liars_poker_bot::algorithms::open_hand_solver::{OpenHandSolver, Optimizations};
use liars_poker_bot::algorithms::pimcts::PIMCTSBot;
use liars_poker_bot::cfragent::cfres::CFRES;
use liars_poker_bot::cfragent::cfrnode::CFRNode;
use liars_poker_bot::cfragent::{CFRAgent, CFRAlgorithm};
use liars_poker_bot::database::memory_node_store::MemoryNodeStore;

use liars_poker_bot::game::bluff::BluffGameState;

use liars_poker_bot::game::euchre::actions::EAction;
use liars_poker_bot::game::euchre::processors::euchre_early_terminate;
use liars_poker_bot::game::euchre::{Euchre, EuchreGameState};
use liars_poker_bot::game::kuhn_poker::KPGameState;
use liars_poker_bot::game::{Action, GameState};

use liars_poker_bot::policy::Policy;
use log::{debug, info, set_max_level, warn, LevelFilter};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use scripts::agent_exploitability::calcualte_agent_exploitability;
use scripts::benchmark::{get_rng, run_benchmark, BenchmarkArgs};
use scripts::estimate_euchre_game_tree::estimate_euchre_game_tree;
use scripts::pass_on_bower::open_hand_score_pass_on_bower;
use scripts::pass_on_bower_alpha::benchmark_pass_on_bower;
use scripts::pass_on_bower_cfr::{run_pass_on_bower_cfr, PassOnBowerCFRArgs};
use scripts::tune::{run_tune, TuneArgs};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};

use crate::scripts::pass_on_bower_cfr::generate_jack_of_spades_deal;

pub mod scripts;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
enum GameType {
    KuhnPoker,
    Euchre,
    Bluff11,
    Bluff21,
    Bluff22,
}

#[derive(Debug, Subcommand, Clone)]
enum Commands {
    Run,
    Benchmark(BenchmarkArgs),
    Analyze,
    Play,
    Scratch,
    Exploitability,
    PassOnBowerOpenHand,
    PassOnBowerAlpha { num_games: usize },
    PassOnBowerCFR(PassOnBowerCFRArgs),
    Tune(TuneArgs),
}

/// Simple program to greet a person
#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, default_value_t = 1)]
    num_games: usize,

    #[clap(long, value_enum, default_value_t=GameType::Euchre)]
    game: GameType,

    #[clap(short)]
    file: Option<String>,

    #[clap(long, value_enum, default_value_t=CFRAlgorithm::CFRCS)]
    alg: CFRAlgorithm,

    /// Allow module to log
    #[structopt(long = "module")]
    modules: Vec<String>,

    #[command(subcommand)]
    command: Commands,

    #[clap(short = 'v', long, action, default_value_t = 1)]
    verbosity: usize,
}

fn main() {
    let args = Args::parse();

    set_max_level(LevelFilter::Info);

    let config = ConfigBuilder::new().set_time_format_rfc3339().build();

    let term_logger_level = match args.verbosity {
        0 => LevelFilter::Error,
        1 => LevelFilter::Warn,
        2 => LevelFilter::Info,
        3 => LevelFilter::Debug,
        4 => LevelFilter::Trace,
        _ => panic!(
            "invalid log level: {}, must be between 0 and 4",
            args.verbosity
        ),
    };

    CombinedLogger::init(vec![
        TermLogger::new(
            term_logger_level,
            config.clone(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            config,
            OpenOptions::new()
                .append(true)
                .write(true)
                .open("liars_poker.log")
                .unwrap(),
        ),
    ])
    .unwrap();

    match args.command {
        Commands::Run => run(args),
        Commands::Benchmark(bench) => run_benchmark(bench),
        Commands::Analyze => run_analyze(args),
        Commands::Play => run_play(args),
        Commands::Scratch => run_scratch(args),
        // Mode::PassOnBowerOpenHand => calculate_open_hand_solver_convergence(args),
        Commands::PassOnBowerOpenHand => open_hand_score_pass_on_bower(args),
        Commands::Exploitability => calcualte_agent_exploitability(args),
        Commands::PassOnBowerAlpha { num_games } => benchmark_pass_on_bower(num_games),
        Commands::Tune(tune) => run_tune(tune),
        Commands::PassOnBowerCFR(bower_cfr) => run_pass_on_bower_cfr(bower_cfr),
    }
}

fn run_scratch(_args: Args) {
    println!("bluff size: {}", mem::size_of::<BluffGameState>());
    println!("kuhn poker size: {}", mem::size_of::<KPGameState>());
    println!("euchre size: {}", mem::size_of::<EuchreGameState>());

    let generator = generate_jack_of_spades_deal;
    let mut alg = CFRES::new_euchre_bidding(generator, get_rng());

    let infostate_path = "infostates.open-hand-20m";
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

fn run_analyze(args: Args) {
    match args.game {
        GameType::KuhnPoker => todo!(),
        GameType::Euchre => estimate_euchre_game_tree(args),
        GameType::Bluff11 => todo!(),
        GameType::Bluff21 => todo!(),
        GameType::Bluff22 => todo!(),
    }
}

fn run(_args: Args) {
    todo!();
    // let _storage = match args.file.unwrap_or("".to_string()).as_str() {
    //     "" => Storage::Temp,
    //     _ => panic!("need to add support to create named files"), // Storage::Named(args.file),
    // };

    // println!("running for: {:?} with {:?}", args.game, args.alg);
    // match args.game {
    //     GameType::KuhnPoker => {
    //         train_cfr_agent(CFRAgent::new(
    //             KuhnPoker::game(),
    //             1,
    //             MemoryNodeStore::default(),
    //             args.alg,
    //         ));
    //     }
    //     GameType::Euchre => {
    //         CFRAgent::new(Euchre::game(), 1, MemoryNodeStore::default(), args.alg).train(10);
    //     }
    //     GameType::Bluff11 => {
    //         train_cfr_agent(CFRAgent::new(
    //             Bluff::game(1, 1),
    //             1,
    //             MemoryNodeStore::default(),
    //             args.alg,
    //         ));
    //     }
    //     GameType::Bluff21 => {
    //         train_cfr_agent(CFRAgent::new(
    //             Bluff::game(2, 1),
    //             1,
    //             MemoryNodeStore::default(),
    //             args.alg,
    //         ));
    //     }
    //     GameType::Bluff22 => {
    //         train_cfr_agent(CFRAgent::new(
    //             Bluff::game(2, 2),
    //             1,
    //             MemoryNodeStore::default(),
    //             args.alg,
    //         ));
    //     }
    // };
}

fn _train_cfr_agent<G: GameState>(mut agent: CFRAgent<G, MemoryNodeStore<CFRNode>>) {
    let mut iteration = 1;

    while iteration < 100_001 {
        agent.train(iteration);
        debug!(
            "finished iteration: {}, starting best response calculation",
            iteration
        );
        let exploitability =
            exploitability::exploitability(agent.game_generator, &mut agent.ns).nash_conv;
        info!(
            "exploitability:\t{}\t{}\t{}",
            iteration,
            agent.nodes_touched(),
            exploitability
        );
        iteration *= 10;
    }
}

fn run_play(_args: Args) {
    let mut rng: StdRng = SeedableRng::seed_from_u64(2);

    let mut agent = PolicyAgent::new(
        PIMCTSBot::new(50, OpenHandSolver::new_euchre(), get_rng()),
        get_rng(),
    );
    let mut player = PlayerAgent::default();
    let user = 0;
    let mut score = [0; 2];
    let mut i = 0;

    loop {
        let mut gs = Euchre::new_state();
        while !gs.is_terminal() {
            if gs.is_chance_node() {
                let actions = actions!(gs);
                let a = *actions.choose(&mut rng).unwrap();
                gs.apply_action(a);
                continue;
            }

            let a = if gs.cur_player() % 2 == (user + i % 2) % 2 {
                player.step(&gs)
            } else {
                agent.step(&gs)
            };

            gs.apply_action(a);
        }
        println!("{}", gs.evaluate(user));
        score[0] += 0.max(gs.evaluate(0) as i8);
        score[1] += 0.max(gs.evaluate(1) as i8);

        println!("{} to {}", score[0], score[1]);
        println!("user is player: {}", user);
        i += 1;
    }
}
