use std::{io, mem};

use clap::{command, Parser, Subcommand, ValueEnum};

use liars_poker_bot::actions;
use liars_poker_bot::agents::{Agent, RandomAgent};

use liars_poker_bot::algorithms::alphamu::AlphaMuBot;
use liars_poker_bot::algorithms::exploitability::{self};

use liars_poker_bot::algorithms::open_hand_solver::OpenHandSolver;
use liars_poker_bot::cfragent::cfrnode::CFRNode;
use liars_poker_bot::cfragent::{CFRAgent, CFRAlgorithm};
use liars_poker_bot::database::memory_node_store::MemoryNodeStore;
use liars_poker_bot::database::Storage;
use liars_poker_bot::game::bluff::{Bluff, BluffGameState};

use liars_poker_bot::game::euchre::{Euchre, EuchreGameState};
use liars_poker_bot::game::kuhn_poker::{KPGameState, KuhnPoker};
use liars_poker_bot::game::{Action, GameState};

use liars_poker_bot::policy::Policy;
use log::{debug, info};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use scripts::agent_exploitability::calcualte_agent_exploitability;
use scripts::benchmark::run_benchmark;
use scripts::estimate_euchre_game_tree::estimate_euchre_game_tree;
use scripts::pass_on_bower::open_hand_score_pass_on_bower;
use scripts::pass_on_bower_alpha::{benchmark_pass_on_bower, tune_alpha_mu};

pub mod scripts;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
enum GameType {
    KuhnPoker,
    Euchre,
    Bluff11,
    Bluff21,
    Bluff22,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Subcommand)]
enum Command {
    Run,
    Benchmark,
    Analyze,
    Play,
    Scratch,
    Exploitability,
    PassOnBowerOpenHand,
    PassOnBowerAlpha,
    TuneAlphaMu { num_games: usize },
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, default_value_t = 1)]
    num_games: usize,

    #[clap(short = 'v', long, action, default_value_t = 0)]
    verbosity: usize,

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
    command: Option<Command>,
}

fn main() {
    let args = Args::parse();

    stderrlog::new()
        .verbosity(args.verbosity)
        .timestamp(stderrlog::Timestamp::Second)
        .show_module_names(true)
        .modules(args.modules.clone())
        .init()
        .unwrap();

    match args.command.unwrap() {
        Command::Run => run(args),
        Command::Benchmark => run_benchmark(args),
        Command::Analyze => run_analyze(args),
        Command::Play => run_play(args),
        Command::Scratch => run_scratch(args),
        // Mode::PassOnBowerOpenHand => calculate_open_hand_solver_convergence(args),
        Command::PassOnBowerOpenHand => open_hand_score_pass_on_bower(args),
        Command::Exploitability => calcualte_agent_exploitability(args),
        Command::PassOnBowerAlpha => benchmark_pass_on_bower(args),
        Command::TuneAlphaMu { num_games: n } => tune_alpha_mu(n),
    }
}

fn run_scratch(_args: Args) {
    println!("bluff size: {}", mem::size_of::<BluffGameState>());
    println!("kuhn poker size: {}", mem::size_of::<KPGameState>());
    println!("euchre size: {}", mem::size_of::<EuchreGameState>());

    let gs = EuchreGameState::from("9s9hTh9dKd|QcJsQsQhAh|KcAcTsAsJh|9cKhJdQdAd|Td|T|9c|");

    let mut alphamu = AlphaMuBot::new(OpenHandSolver::new(), 3, 2, SeedableRng::seed_from_u64(42));
    let policy = alphamu.action_probabilities(&gs);

    for _ in 0..1 {
        let mut alphamu =
            AlphaMuBot::new(OpenHandSolver::new(), 3, 2, SeedableRng::seed_from_u64(42));
        alphamu.use_optimizations = false;
        info!("starting call for non-optimized alphamu");
        assert_eq!(alphamu.action_probabilities(&gs), policy);
    }
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

fn run(args: Args) {
    let _storage = match args.file.unwrap_or("".to_string()).as_str() {
        "" => Storage::Temp,
        _ => panic!("need to add support to create named files"), // Storage::Named(args.file),
    };

    println!("running for: {:?} with {:?}", args.game, args.alg);
    match args.game {
        GameType::KuhnPoker => {
            train_cfr_agent(CFRAgent::new(
                KuhnPoker::game(),
                1,
                MemoryNodeStore::default(),
                args.alg,
            ));
        }
        GameType::Euchre => {
            CFRAgent::new(Euchre::game(), 1, MemoryNodeStore::default(), args.alg).train(10);
        }
        GameType::Bluff11 => {
            train_cfr_agent(CFRAgent::new(
                Bluff::game(1, 1),
                1,
                MemoryNodeStore::default(),
                args.alg,
            ));
        }
        GameType::Bluff21 => {
            train_cfr_agent(CFRAgent::new(
                Bluff::game(2, 1),
                1,
                MemoryNodeStore::default(),
                args.alg,
            ));
        }
        GameType::Bluff22 => {
            train_cfr_agent(CFRAgent::new(
                Bluff::game(2, 2),
                1,
                MemoryNodeStore::default(),
                args.alg,
            ));
        }
    };
}

fn train_cfr_agent<G: GameState>(mut agent: CFRAgent<G, MemoryNodeStore<CFRNode>>) {
    let mut iteration = 1;

    while iteration < 100_001 {
        agent.train(iteration);
        debug!(
            "finished iteration: {}, starting best response calculation",
            iteration
        );
        let exploitability =
            exploitability::exploitability(agent.game.clone(), &mut agent.ns).nash_conv;
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
    let mut gs = Euchre::new_state();
    let mut rng: StdRng = SeedableRng::seed_from_u64(1);

    let mut agent = RandomAgent::new();
    let user = 0;

    while !gs.is_terminal() {
        if gs.is_chance_node() {
            let actions = actions!(gs);
            let a = *actions.choose(&mut rng).unwrap();
            gs.apply_action(a);
            continue;
        }

        let a = if gs.cur_player() == user {
            handle_player_turn(&mut gs)
        } else {
            agent.step(&gs)
        };

        let cur_player = gs.cur_player();
        gs.apply_action(a);
        println!("{}: {}", cur_player, gs.istate_string(user));
    }

    todo!()
}

fn handle_player_turn<T: GameState>(gs: &mut T) -> Action {
    let player = gs.cur_player();
    println!("{}", gs.istate_string(player));
    println!("{:?}", actions!(gs));

    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .expect("Failed to read input");

    todo!()
    // return buffer.trim().parse().expect("Failed to parse digits");
}
