use std::{io, mem};

use clap::{command, Parser, Subcommand, ValueEnum};

use itertools::Itertools;
use liars_poker_bot::actions;
use liars_poker_bot::agents::{Agent, PlayerAgent, PolicyAgent};

use liars_poker_bot::algorithms::exploitability::{self};

use liars_poker_bot::algorithms::ismcts::Evaluator;
use liars_poker_bot::algorithms::open_hand_solver::{OpenHandSolver, Optimizations};
use liars_poker_bot::algorithms::pimcts::PIMCTSBot;
use liars_poker_bot::cfragent::cfrnode::CFRNode;
use liars_poker_bot::cfragent::{CFRAgent, CFRAlgorithm};
use liars_poker_bot::database::memory_node_store::MemoryNodeStore;
use liars_poker_bot::database::Storage;
use liars_poker_bot::game::bluff::{Bluff, BluffGameState};

use liars_poker_bot::game::euchre::actions::EAction;
use liars_poker_bot::game::euchre::processors::euchre_early_terminate;
use liars_poker_bot::game::euchre::{Euchre, EuchreGameState};
use liars_poker_bot::game::kuhn_poker::{KPGameState, KuhnPoker};
use liars_poker_bot::game::{Action, GameState};

use liars_poker_bot::policy::Policy;
use log::{debug, info};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use scripts::agent_exploitability::calcualte_agent_exploitability;
use scripts::benchmark::{get_rng, run_benchmark, BenchmarkArgs};
use scripts::estimate_euchre_game_tree::estimate_euchre_game_tree;
use scripts::pass_on_bower::open_hand_score_pass_on_bower;
use scripts::pass_on_bower_alpha::benchmark_pass_on_bower;
use scripts::tune::{run_tune, TuneArgs};

pub mod scripts;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, ValueEnum)]
enum GameType {
    KuhnPoker,
    Euchre,
    Bluff11,
    Bluff21,
    Bluff22,
}

#[derive(Debug, Subcommand, Copy, Clone)]
enum Commands {
    Run,
    Benchmark(BenchmarkArgs),
    Analyze,
    Play,
    Scratch,
    Exploitability,
    PassOnBowerOpenHand,
    PassOnBowerAlpha { num_games: usize },
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

    #[clap(short = 'v', long, action, default_value_t = 0)]
    verbosity: usize,
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
    }
}

fn run_scratch(_args: Args) {
    println!("bluff size: {}", mem::size_of::<BluffGameState>());
    println!("kuhn poker size: {}", mem::size_of::<KPGameState>());
    println!("euchre size: {}", mem::size_of::<EuchreGameState>());

    let gs = EuchreGameState::from("Qc9sTs9dAd|Tc9hAhTdJd|9cKcAcJhQh|JcJsKsAsKd|Qs|PT|");
    // println!("{:?}", gs);
    // println!(
    //     "{:?}",
    //     gs.istate_key(3)
    //         .iter()
    //         .map(|x| EAction::from(*x))
    //         .collect_vec()
    // );

    let actions = actions!(gs).into_iter().map(EAction::from).collect_vec();
    println!("actions: {:?}", actions);

    let mut agent = PIMCTSBot::new(
        50,
        OpenHandSolver::default(),
        SeedableRng::seed_from_u64(43),
    );
    let policy = agent.action_probabilities(&gs);

    for (a, p) in policy.to_vec() {
        println!("{} ({}): {}", EAction::from(a), a, p);
    }

    println!(
        "euchre: {}",
        OpenHandSolver::new(Optimizations {
            use_transposition_table: true,
            isometric_transposition: true,
            max_depth_for_tt: 255,
            action_processor: |_: &EuchreGameState, _: &mut Vec<Action>| {},
            can_early_terminate: euchre_early_terminate
        })
        .evaluate_player(&gs, 0)
    );
    println!(
        "default: {}",
        OpenHandSolver::default().evaluate_player(&gs, 0)
    );

    println!(
        "no cache: {}",
        OpenHandSolver::new_without_cache().evaluate_player(&gs, 0)
    );
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
