use std::collections::HashMap;

use clap::Parser;

use clap::clap_derive::ArgEnum;

use liars_poker_bot::agents::{Agent, RandomAgent};
use liars_poker_bot::cfragent::CFRAgent;
use liars_poker_bot::database::Storage;
use liars_poker_bot::euchre::Euchre;
use liars_poker_bot::game::{run_game, GameState};
use rand::thread_rng;

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
}

fn main() {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    match args.mode {
        Mode::Run => run(args),
        Mode::Benchmark => run_benchmark(args),
        Mode::Analyze => run_analyze(args),
    }
}

fn run_analyze(args: Args) {
    assert_eq!(args.game, GameType::Euchre);

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
}

fn run(args: Args) {
    let storage = match args.file.as_str() {
        "" => Storage::Tempfile,
        _ => Storage::Namedfile(args.file),
    };
    let _cfr = CFRAgent::new(Euchre::game(), 0, 2, storage);
}

fn run_benchmark(args: Args) {
    let g = match args.game {
        GameType::Euchre => Box::new(|| -> Box<dyn GameState> { Box::new(Euchre::new_state()) }),
        _ => todo!(),
    };

    let cfr = CFRAgent::new(Euchre::game(), 0, 2, Storage::Tempfile);
    let mut agents: Vec<Box<dyn Fn() -> Box<dyn Agent>>> = Vec::new();
    agents.push(Box::new(|| -> Box<dyn Agent> {
        Box::new(RandomAgent::new())
    }));

    agents.push(Box::new(|| -> Box<dyn Agent> { Box::new(cfr.clone()) }));

    let mut rng = thread_rng();
    for p0 in 0..agents.len() {
        for p1 in 0..agents.len() {
            let mut score = [0.0; 2];
            for _ in 0..args.num_games {
                let mut s = g();
                run_game(
                    s.as_mut(),
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
