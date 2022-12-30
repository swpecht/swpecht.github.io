pub mod agents;
pub mod cfragent;
pub mod euchre;
pub mod game;
pub mod kuhn_poker;

use agents::{Agent, RandomAgent};
use clap::Parser;

use clap::clap_derive::ArgEnum;

use cfragent::CFRAgent;
use euchre::Euchre;
use game::{run_game, GameState};
use kuhn_poker::KuhnPoker;
use rand::thread_rng;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum GameType {
    KP,
    Euchre,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value_t = 5)]
    num_games: usize,

    #[clap(short, long, action, default_value_t = 0)]
    verbosity: usize,

    #[clap(short, long, action)]
    benchmark: bool,

    #[clap(arg_enum, value_parser, default_value_t = GameType::KP)]
    game: GameType,
}

fn main() {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .verbosity(args.verbosity)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    if args.benchmark {
        let g = match args.game {
            GameType::Euchre => Box::new(|| -> Box<dyn GameState> { Box::new(Euchre::new()) }),
            _ => todo!(),
        };

        let mut agents: Vec<Box<dyn Fn() -> Box<dyn Agent>>> = Vec::new();
        agents.push(Box::new(|| -> Box<dyn Agent> {
            Box::new(RandomAgent::new())
        }));

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
    } else {
        let cfr = CFRAgent::new(0, 100000);
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
                    let mut g = KuhnPoker::new();
                    run_game(
                        &mut g,
                        &mut vec![agents[p0]().as_mut(), agents[p1]().as_mut()],
                        &mut rng,
                    );
                    let result = g.evaluate();
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
}

#[macro_export]
macro_rules! agents {
    ( $game:ty, $( $x:expr ),* ) => {
        {
            let mut temp_vec: Vec<fn(&$game) -> Box<dyn Agent<$game>>> = Vec::new();
            $(
                temp_vec.push(|g: &$game| -> Box<dyn Agent<$game>> {
                    Box::new($x(g))
                });
            )*
            temp_vec
        }
    };
}
