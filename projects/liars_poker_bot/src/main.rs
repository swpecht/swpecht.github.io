use std::{io, mem};

use clap::Parser;

use clap::clap_derive::ArgEnum;

use liars_poker_bot::actions;
use liars_poker_bot::agents::{Agent, RandomAgent};

use liars_poker_bot::algorithms::exploitability::{self};

use liars_poker_bot::cfragent::cfrnode::CFRNode;
use liars_poker_bot::cfragent::{CFRAgent, CFRAlgorithm};
use liars_poker_bot::database::memory_node_store::MemoryNodeStore;
use liars_poker_bot::database::Storage;
use liars_poker_bot::game::bluff::{Bluff, BluffGameState};

use liars_poker_bot::game::euchre::{Euchre, EuchreGameState};
use liars_poker_bot::game::kuhn_poker::{KPGameState, KuhnPoker};
use liars_poker_bot::game::{Action, GameState};

use log::{debug, info};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use scripts::agent_exploitability::calcualte_agent_exploitability;
use scripts::benchmark::run_benchmark;
use scripts::estimate_euchre_game_tree::estimate_euchre_game_tree;
use scripts::pass_on_bower::calculate_open_hand_solver_convergence;
use scripts::pass_on_bower_alpha::benchmark_pass_on_bower;

pub mod scripts;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum GameType {
    KuhnPoker,
    Euchre,
    Bluff11,
    Bluff21,
    Bluff22,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum Mode {
    Run,
    Benchmark,
    Analyze,
    Play,
    Scratch,
    Exploitability,
    PassOnBowerOpenHand,
    PassOnBowerAlpha,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[clap(short, long, value_parser, default_value_t = 1)]
    num_games: usize,

    #[clap(short, long, action, default_value_t = 0)]
    verbosity: usize,

    #[clap(arg_enum, long, value_parser, default_value_t = Mode::Run)]
    mode: Mode,

    #[clap(arg_enum, value_parser, default_value_t = GameType::Bluff11)]
    game: GameType,

    #[clap(short, long, action, default_value = "")]
    file: String,

    #[clap(arg_enum, long, value_parser, default_value_t = CFRAlgorithm::CFRCS)]
    alg: CFRAlgorithm,

    /// Allow module to log
    #[structopt(long = "module")]
    modules: Vec<String>,
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

    match args.mode {
        Mode::Run => run(args),
        Mode::Benchmark => run_benchmark(args),
        Mode::Analyze => run_analyze(args),
        Mode::Play => run_play(args),
        Mode::Scratch => run_scratch(args),
        Mode::PassOnBowerOpenHand => calculate_open_hand_solver_convergence(args),
        Mode::Exploitability => calcualte_agent_exploitability(args),
        Mode::PassOnBowerAlpha => benchmark_pass_on_bower(args),
        // Mode::PassOnBowerOpenHand => open_hand_score_pass_on_bower(args),
    }
}

fn run_scratch(_args: Args) {
    println!("bluff size: {}", mem::size_of::<BluffGameState>());
    println!("kuhn poker size: {}", mem::size_of::<KPGameState>());
    println!("euchre size: {}", mem::size_of::<EuchreGameState>());

    // let gs = EuchreGameState::from("TCQCQHAHTD|9HKHJDKDAD|AC9SQSTHJH|9CJCKCJSQD|AS|");
    // let mut gs = Bluff::new_state(1, 1);
    // gs.apply_action(BluffActions::Roll(Dice::Two).into());
    // gs.apply_action(BluffActions::Roll(Dice::Wild).into());

    // let mut agent = AlphaMuBot::new(OpenHandSolver::new(), 10, 10);
    // agent.run_search(&gs);
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
    let _storage = match args.file.as_str() {
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
