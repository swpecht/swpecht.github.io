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

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value_t = 1)]
    num_games: usize,

    #[clap(short, long, action, default_value_t = 0)]
    verbosity: usize,

    #[clap(short, long, action)]
    benchmark: bool,

    #[clap(arg_enum, value_parser, default_value_t = GameType::KP)]
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

    if args.benchmark {
        let g = match args.game {
            GameType::Euchre => {
                Box::new(|| -> Box<dyn GameState> { Box::new(Euchre::new_state()) })
            }
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
    } else {
        let storage = match args.file.as_str() {
            "" => Storage::Tempfile,
            _ => Storage::Namedfile(args.file),
        };
        let _cfr = CFRAgent::new(Euchre::game(), 0, 2, storage);
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
