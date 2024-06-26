use std::fs::OpenOptions;
use std::io::Read;
use std::mem;
use std::path::Path;

use card_platypus::algorithms::cfres::{self, InfoState};

use card_platypus::database::indexer::Indexer;
use card_platypus::database::NodeStore;
use clap::{command, Parser, Subcommand, ValueEnum};

use dashmap::iter;
use games::gamestates::bluff::BluffGameState;
use games::gamestates::euchre::actions::EAction;
use games::gamestates::euchre::iterator::EuchreIsomorphicIStateIterator;
use games::gamestates::euchre::EuchreGameState;
use games::gamestates::kuhn_poker::KPGameState;
use games::istate::IStateKey;
use games::translate_istate;
use log::{set_max_level, LevelFilter};

use scripts::agent_exploitability::calcualte_agent_exploitability;
use scripts::benchmark::{run_benchmark, BenchmarkArgs};
use scripts::estimate_euchre_game_tree::estimate_euchre_game_tree;
use scripts::pass_on_bower::open_hand_score_pass_on_bower;
use scripts::pass_on_bower_alpha::benchmark_pass_on_bower;
use scripts::pass_on_bower_cfr::{
    analyze_istate, parse_weights, run_pass_on_bower_cfr, PassOnBowerCFRArgs,
};
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

    println!("cfres node {}", mem::size_of::<InfoState>());
    println!("istate key {}", mem::size_of::<IStateKey>());

    // let a = cfres::CFRES::new_euchre(
    //     scripts::benchmark::get_rng(),
    //     3,
    //     Some(Path::new(
    //         "/var/lib/card_platypus/infostate.three_card_played_f32/",
    //     )),
    // );
    // a.save().unwrap();
    // println!(
    //     "index size: {}, infostates: {}",
    //     a.indexer_size(),
    //     a.num_info_states()
    // );
    let database = NodeStore::new_euchre(
        Some(Path::new(
            "/var/lib/card_platypus/infostate.three_card_played_f32/",
        )),
        3,
    )
    .unwrap();

    println!(
        "infostates: {}, indexsize: {}",
        database.len(),
        database.indexer_len()
    );
    println!("finding missing istates");
    let mut count = 0;
    let iterator = EuchreIsomorphicIStateIterator::with_face_up(3, &[EAction::NS]);
    for istate in iterator {
        if database.get(&istate).is_none() {
            println!("{:?}", translate_istate!(istate, EAction));
            count += 1;
        }
        if count >= 20 {
            break;
        }
    }

    // for i in 0..5 {
    //     let n = EuchreIsomorphicIStateIterator::with_face_up(i, &[EAction::NS]).count();
    //     println!("istates {}: {}", i, n);
    // }
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

fn run_play(_args: Args) {
    todo!()
}
