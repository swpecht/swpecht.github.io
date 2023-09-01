use std::fs::OpenOptions;
use std::mem;

use card_platypus::algorithms::ismcts::ResampleFromInfoState;

use card_platypus::cfragent::cfres::InfoState;
use card_platypus::istate::IStateKey;
use clap::{command, Parser, Subcommand, ValueEnum};

use card_platypus::actions;
use card_platypus::agents::{Agent, PlayerAgent, PolicyAgent};

use card_platypus::algorithms::exploitability::{self};

use card_platypus::algorithms::open_hand_solver::OpenHandSolver;
use card_platypus::algorithms::pimcts::PIMCTSBot;
use card_platypus::cfragent::cfrnode::CFRNode;
use card_platypus::cfragent::{cfres, CFRAgent, CFRAlgorithm};
use card_platypus::database::memory_node_store::MemoryNodeStore;

use card_platypus::game::bluff::BluffGameState;

use card_platypus::game::euchre::{Euchre, EuchreGameState};
use card_platypus::game::kuhn_poker::KPGameState;
use card_platypus::game::GameState;

use log::{debug, info, set_max_level, LevelFilter};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};
use scripts::agent_exploitability::calcualte_agent_exploitability;
use scripts::benchmark::{get_rng, run_benchmark, BenchmarkArgs};
use scripts::estimate_euchre_game_tree::estimate_euchre_game_tree;
use scripts::pass_on_bower::open_hand_score_pass_on_bower;
use scripts::pass_on_bower_alpha::benchmark_pass_on_bower;
use scripts::pass_on_bower_cfr::{
    analyze_istate, parse_weights, run_pass_on_bower_cfr, PassOnBowerCFRArgs,
};
use scripts::tune::{run_tune, TuneArgs};
use simplelog::{
    ColorChoice, CombinedLogger, ConfigBuilder, TermLogger, TerminalMode, WriteLogger,
};

use crate::scripts::config::train_cfr_from_config;

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
    EuchreCFRTrain { profile: String },
    PassOnBowerCFRTrain(PassOnBowerCFRArgs),
    PassOnBowerCFRParseWeights { infostate_path: String },
    PassOnBowerCFRAnalyzeIstate { num_games: usize },
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
    // Set feature flags
    cfres::feature::enable(cfres::feature::NormalizeSuit);
    cfres::feature::enable(cfres::feature::LinearCFR);
    cfres::feature::disable(cfres::feature::SingleThread);

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
                .create(true)
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
        Commands::PassOnBowerCFRTrain(bower_cfr) => run_pass_on_bower_cfr(bower_cfr),
        Commands::PassOnBowerCFRParseWeights { infostate_path } => {
            parse_weights(infostate_path.as_str())
        }
        Commands::PassOnBowerCFRAnalyzeIstate { num_games } => analyze_istate(num_games),
        Commands::EuchreCFRTrain { profile } => train_cfr_from_config(profile.as_str()).unwrap(),
    }
}

fn run_scratch(_args: Args) {
    println!("bluff size: {}", mem::size_of::<BluffGameState>());
    println!("kuhn poker size: {}", mem::size_of::<KPGameState>());
    println!("euchre size: {}", mem::size_of::<EuchreGameState>());

    println!("cfres node {}", mem::size_of::<usize>());

    train_cfr_from_config("baseline").unwrap();
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
