use std::io;

use clap::Parser;

use clap::clap_derive::ArgEnum;

use liars_poker_bot::agents::{Agent, RandomAgent};
use liars_poker_bot::cfragent::CFRAgent;
use liars_poker_bot::database::memory_node_store::MemoryNodeStore;
use liars_poker_bot::database::{tune_page, Storage};
use liars_poker_bot::game::bluff::Bluff;
use liars_poker_bot::game::euchre::{Euchre, EuchreGameState};
use liars_poker_bot::game::{run_game, Action, GameState};

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{thread_rng, SeedableRng};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum GameType {
    KP,
    Euchre,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum Mode {
    Run,
    Benchmark,
    Analyze,
    Play,
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

    #[clap(arg_enum, value_parser, default_value_t = GameType::Euchre)]
    game: GameType,

    #[clap(short, long, action, default_value = "")]
    file: String,

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
    }
}

fn run_analyze(args: Args) {
    assert_eq!(args.game, GameType::Euchre);

    tune_page::tune_page_size();

    let mut total_end_states = 0;
    let mut total_states = 0;
    let mut total_rounds = 0;
    let mut children = [0.0; 28];
    let runs = 10000;
    let mut agent = RandomAgent::new();

    for _ in 0..runs {
        let mut round = 0;
        let mut end_states = 1;
        let mut s = Euchre::new_state();
        while !s.is_terminal() {
            if s.is_chance_node() {
                let a = agent.step(&s);
                s.apply_action(a);
            } else {
                let legal_move_count = s.legal_actions().len();
                end_states *= legal_move_count;
                total_states = total_states + end_states;
                children[round] += legal_move_count as f64;
                round += 1;
                let a = agent.step(&s);
                s.apply_action(a);
            }
        }
        total_end_states += end_states;
        total_rounds += round;
    }

    println!("average post deal end states: {}", total_end_states / runs);
    println!("average post deal states: {}", total_states / runs);
    println!("rounds: {}", total_rounds / runs);
    let mut sum = 1.0;
    for i in 0..children.len() {
        println!(
            "round {} has {} children, {} peers",
            i,
            children[i] / runs as f64,
            sum
        );
        sum *= (children[i] / runs as f64).max(1.0);
    }

    // traverse gametress
    let mut s = Euchre::new_state();
    // let mut s = KuhnPoker::new_state();

    // TODO: A seed of 0 here seems to break things. Why?
    let mut rng: StdRng = SeedableRng::seed_from_u64(0);
    while s.is_chance_node() {
        let a = *s.legal_actions().choose(&mut rng).unwrap();
        s.apply_action(a);
    }

    println!("total storable nodes: {}", traverse_game_tree(s, 0));
}

fn traverse_game_tree<T: GameState>(s: T, depth: usize) -> usize {
    if s.is_terminal() {
        return 0; // don't need to store leaf node
    }

    let mut count = 1;
    for a in s.legal_actions() {
        if depth <= 2 {
            println!("depth: {}, nodes: {}", depth, count)
        }

        let mut new_s = s.clone();
        new_s.apply_action(a);

        // don't need to store if only 1 action
        while new_s.legal_actions().len() == 1 {
            new_s.apply_action(new_s.legal_actions()[0])
        }

        count += traverse_game_tree(new_s, depth + 1);
    }

    return count;
}

fn run(args: Args) {
    let _storage = match args.file.as_str() {
        "" => Storage::Temp,
        _ => panic!("need to add support to create named files"), // Storage::Named(args.file),
    };
    let _cfr = CFRAgent::new(Bluff::game(), 1, 5000, MemoryNodeStore::new());
}

fn run_benchmark(args: Args) {
    let g = match args.game {
        GameType::Euchre => Box::new(|| -> EuchreGameState { Euchre::new_state() }),
        _ => todo!(),
    };

    // let cfr = CFRAgent::new(Euchre::game(), 0, 2, Storage::Temp);
    let mut agents: Vec<Box<dyn Fn() -> Box<dyn Agent<EuchreGameState>>>> = Vec::new();
    agents.push(Box::new(|| -> Box<dyn Agent<EuchreGameState>> {
        Box::new(RandomAgent::new())
    }));

    // agents.push(Box::new(|| -> Box<dyn Agent<EuchreGameState>> {
    //     Box::new(cfr.clone())
    // }));

    let mut rng = thread_rng();
    for p0 in 0..agents.len() {
        for p1 in 0..agents.len() {
            let mut score = [0.0; 2];
            for _ in 0..args.num_games {
                let mut s = g();
                run_game(
                    &mut s,
                    &mut vec![
                        agents[p0]().as_mut(),
                        agents[p1]().as_mut(),
                        agents[p0]().as_mut(),
                        agents[p1]().as_mut(),
                    ],
                    &mut rng,
                );
                let result = s.evaluate();
                score[0] += result[0];
                score[1] += result[1];
            }
            println!(
                "{} vs {}: {} to {}",
                agents[p0]().get_name(),
                agents[p1]().get_name(),
                score[0],
                score[1]
            )
        }
    }
}

fn run_play(_args: Args) {
    let mut s = Euchre::new_state();
    let mut rng: StdRng = SeedableRng::seed_from_u64(1);

    let mut agent = RandomAgent::new();
    let user = 0;

    while !s.is_terminal() {
        if s.is_chance_node() {
            let actions = s.legal_actions();
            let a = *actions.choose(&mut rng).unwrap();
            s.apply_action(a);
            continue;
        }

        let a;
        if s.cur_player() == user {
            a = handle_player_turn(&mut s);
        } else {
            a = agent.step(&s);
        }

        let cur_player = s.cur_player();
        s.apply_action(a);
        println!("{}: {}", cur_player, s.istate_string(user));
    }

    todo!()
}

fn handle_player_turn<T: GameState>(s: &mut T) -> Action {
    let player = s.cur_player();
    println!("{}", s.istate_string(player));
    println!("{:?}", s.legal_actions());

    let mut buffer = String::new();
    io::stdin()
        .read_line(&mut buffer)
        .expect("Failed to read input");

    return buffer.trim().parse().expect("Failed to parse digits");
}
