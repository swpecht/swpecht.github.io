use std::collections::HashMap;
use std::{io, mem};

use clap::Parser;

use clap::clap_derive::ArgEnum;

use itertools::Itertools;
use liars_poker_bot::actions;
use liars_poker_bot::agents::{Agent, RandomAgent};

use liars_poker_bot::algorithms::alphamu::AlphaMuBot;
use liars_poker_bot::algorithms::exploitability::{self};
use liars_poker_bot::algorithms::ismcts::{
    Evaluator, ISMCTBotConfig, ISMCTSBot, RandomRolloutEvaluator, ResampleFromInfoState,
};
use liars_poker_bot::algorithms::open_hand_solver::{alpha_beta_search, OpenHandSolver};
use liars_poker_bot::cfragent::cfrnode::CFRNode;
use liars_poker_bot::cfragent::{CFRAgent, CFRAlgorithm};
use liars_poker_bot::database::memory_node_store::MemoryNodeStore;
use liars_poker_bot::database::Storage;
use liars_poker_bot::game::bluff::{Bluff, BluffGameState};
use liars_poker_bot::game::euchre::actions::{Card, EAction};
use liars_poker_bot::game::euchre::{Euchre, EuchreGameState};
use liars_poker_bot::game::kuhn_poker::{KPGameState, KuhnPoker};
use liars_poker_bot::game::{run_game, Action, Game, GameState};

use liars_poker_bot::policy::Policy;
use log::{debug, info, trace};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};

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
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
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
    }
}

fn run_scratch(_args: Args) {
    println!("bluff size: {}", mem::size_of::<BluffGameState>());
    println!("kuhn poker size: {}", mem::size_of::<KPGameState>());
    println!("euchre size: {}", mem::size_of::<EuchreGameState>());

    let mut rng: StdRng = SeedableRng::seed_from_u64(42);
    let mut evaluator = OpenHandSolver::new(100, rng.clone());

    // for _ in 0..1 {
    //     let mut gs = Euchre::new_state();
    //     while gs.is_chance_node() {
    //         let a = *actions!(gs).choose(&mut rng).unwrap();
    //         gs.apply_action(a)
    //     }

    //     info!(
    //         "Evaluator for {}: {:?}",
    //         gs.istate_string(gs.cur_player()),
    //         evaluator.evaluate(&gs)
    //     );
    //     while !gs.is_terminal() {
    //         let cur_player = gs.cur_player();
    //         let (v, a) = alpha_beta_search(gs.clone(), cur_player);
    //         info!(
    //             "{}: {}: value: {}, action: {}",
    //             gs,
    //             cur_player,
    //             v,
    //             EAction::from(a.unwrap())
    //         );
    //         gs.apply_action(a.unwrap());
    //     }

    //     info!("p0, p1 value: {}, {}", gs.evaluate(0), gs.evaluate(1));
    // }

    info!("iterating through pass on the bower nodes");
    for gs in PassOnBowerIterator::new() {
        trace!("processing node: {}", gs);
    }

    info!("calculating evaluator converge");
    for i in 0..50 {
        let mut gs = Euchre::new_state();
        while gs.is_chance_node() {
            let a = *actions!(gs).choose(&mut rng).unwrap();
            gs.apply_action(a)
        }

        for rollouts in [1, 10, 20, 100] {
            evaluator.set_rollout(rollouts);
            let policy = evaluator.action_probabilities(&gs);
            info!(
                "{}\t{}\t{}\t{}\t{}",
                i,
                rollouts,
                gs,
                policy[EAction::Pass.into()],
                policy[EAction::Pickup.into()]
            );
        }
    }
}

struct PassOnBowerIterator {
    hands: Vec<[EAction; 5]>,
}

impl PassOnBowerIterator {
    fn new() -> Self {
        let mut hands = Vec::new();
        // todo: rewrite with combination function?
        for a in 0..20 {
            for b in a + 1..21 {
                for c in b + 1..22 {
                    for d in c + 1..23 {
                        for e in d + 1..24 {
                            if a == Card::JS.into()
                                || b == Card::JS.into()
                                || c == Card::JS.into()
                                || d == Card::JS.into()
                                || e == Card::JS.into()
                            {
                                continue;
                            }
                            hands.push([
                                EAction::DealPlayer { c: a.into() },
                                EAction::DealPlayer { c: b.into() },
                                EAction::DealPlayer { c: c.into() },
                                EAction::DealPlayer { c: d.into() },
                                EAction::DealPlayer { c: e.into() },
                            ])
                        }
                    }
                }
            }
        }
        Self { hands }
    }
}

impl Iterator for PassOnBowerIterator {
    type Item = EuchreGameState;

    fn next(&mut self) -> Option<Self::Item> {
        let jack = EAction::DealPlayer { c: Card::JS };
        if let Some(hand) = self.hands.pop() {
            let mut gs = Euchre::new_state();
            while gs.cur_player() != 3 {
                let actions = actions!(gs);
                for a in actions {
                    if !hand.contains(&a.into()) && EAction::from(a) != jack {
                        gs.apply_action(a);
                        break;
                    }
                }
            }

            // deal the dealers hands
            for c in hand {
                gs.apply_action(c.into())
            }

            // deal the faceup card
            gs.apply_action(EAction::DealFaceUp { c: Card::JS }.into());

            return Some(gs);
        }

        None
    }
}

fn run_analyze(args: Args) {
    assert_eq!(args.game, GameType::Euchre);

    let mut total_end_states = 0;
    let mut total_states = 0;
    let mut _total_rounds = 0;
    let mut children = [0.0; 28];
    let runs = 10000;
    let mut agent = RandomAgent::new();

    for _ in 0..runs {
        let mut round = 0;
        let mut end_states = 1;
        let mut gs = Euchre::new_state();
        while !gs.is_terminal() {
            if gs.is_chance_node() {
                let a = agent.step(&gs);
                gs.apply_action(a);
            } else {
                let legal_move_count = actions!(gs).len();
                end_states *= legal_move_count;
                total_states += end_states;
                children[round] += legal_move_count as f64;
                round += 1;
                let a = agent.step(&gs);
                gs.apply_action(a);
            }
        }
        total_end_states += end_states;
        _total_rounds += round;
    }

    println!("average post deal end states: {}", total_end_states / runs);
    println!("average post deal states: {}", total_states / runs);
    // println!("rounds: {}", total_rounds / runs);
    // let mut sum = 1.0;
    // for (i, c) in children.iter().enumerate() {
    //     println!(
    //         "round {} has {} children, {} peers",
    //         i,
    //         c / runs as f64,
    //         sum
    //     );
    //     sum *= (c / runs as f64).max(1.0);
    // }

    // traverse gametress
    let mut gs = Euchre::new_state();
    // let mut s = KuhnPoker::new_state();

    // TODO: A seed of 0 here seems to break things. Why?
    let mut rng: StdRng = SeedableRng::seed_from_u64(0);
    while gs.is_chance_node() {
        let a = *actions!(gs).choose(&mut rng).unwrap();
        gs.apply_action(a);
    }

    println!("total storable nodes: {}", traverse_game_tree(gs, 0));
}

fn traverse_game_tree<T: GameState>(gs: T, depth: usize) -> usize {
    if gs.is_terminal() {
        return 0; // don't need to store leaf node
    }

    let mut count = 1;
    for a in actions!(gs) {
        if depth <= 2 {
            println!("depth: {}, nodes: {}", depth, count)
        }

        let mut new_s = gs.clone();
        new_s.apply_action(a);

        // don't need to store if only 1 action
        while actions!(new_s).len() == 1 {
            new_s.apply_action(actions!(new_s)[0])
        }

        count += traverse_game_tree(new_s, depth + 1);
    }

    count
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

fn run_benchmark(args: Args) {
    match args.game {
        GameType::KuhnPoker => run_benchmark_for_game(args, KuhnPoker::game()),
        GameType::Euchre => run_benchmark_for_game(args, Euchre::game()),
        GameType::Bluff11 => todo!(),
        GameType::Bluff21 => todo!(),
        GameType::Bluff22 => todo!(),
    }
}

fn run_benchmark_for_game<G: GameState + ResampleFromInfoState>(args: Args, game: Game<G>) {
    let mut agents: HashMap<String, &mut dyn Agent<G>> = HashMap::new();
    let ra: &mut dyn Agent<G> = &mut RandomAgent::new();
    agents.insert(ra.get_name(), ra);

    let config = ISMCTBotConfig::default();
    let ismcts = &mut ISMCTSBot::new(
        game.clone(),
        1.5,
        100,
        RandomRolloutEvaluator::new(100, SeedableRng::seed_from_u64(1)),
        config,
    );
    agents.insert(ismcts.get_name(), ismcts);

    let alphamu = &mut AlphaMuBot::new(
        RandomRolloutEvaluator::new(100, SeedableRng::seed_from_u64(1)),
        30,
        30,
    );
    agents.insert(alphamu.get_name(), alphamu);

    let mut cfr = CFRAgent::new(
        game.clone(),
        42,
        MemoryNodeStore::default(),
        CFRAlgorithm::CFRCS,
    );
    cfr.train(10000);
    agents.insert(cfr.get_name(), &mut cfr);

    let agent_names = agents.keys().cloned().collect_vec();

    for a1_name in agent_names.clone() {
        for a2_name in agent_names.clone() {
            let a1 = agents.remove(&a1_name).unwrap();
            let mut a2 = if a1_name != a2_name {
                Some(agents.remove(&a2_name).unwrap())
            } else {
                None
            };

            let mut returns = vec![0.0; game.max_players];
            for _ in 0..args.num_games {
                let r = run_game(&mut (game.new)(), a1, &mut a2, &mut thread_rng());
                for (i, v) in r.iter().enumerate() {
                    returns[i] += v;
                }
            }
            println!(
                "{:?}\t{:?}\t{}\t{}",
                a1_name, a2_name, returns[0], returns[1]
            );

            agents.insert(a1_name.clone(), a1);
            if a1_name != a2_name {
                agents.insert(a2_name.clone(), a2.unwrap());
            }
        }
    }

    todo!("implement exploitability calculation")
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
