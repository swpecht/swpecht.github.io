pub mod agents;
pub mod game;
pub mod kuhn_poker;

use agents::RandomAgent;
use clap::Parser;

use clap::clap_derive::ArgEnum;

use game::{run_game, GameState};
use kuhn_poker::KuhnPoker;
use rand::thread_rng;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum, Debug)]
enum GameType {
    KP,
}

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value_t = 5)]
    num_games: usize,

    #[clap(short, long, action)]
    quiet: bool,

    #[clap(short, long, action)]
    benchmark: bool,

    #[clap(arg_enum, value_parser, default_value_t = GameType::KP)]
    game: GameType,
}

fn main() {
    let args = Args::parse();

    stderrlog::new()
        .module(module_path!())
        .quiet(args.quiet)
        .verbosity(log::Level::Debug)
        .timestamp(stderrlog::Timestamp::Second)
        .init()
        .unwrap();

    if args.benchmark {
        todo!()
    } else {
        let mut score = [0.0; 2];
        for _ in 0..args.num_games {
            let mut g = KuhnPoker::new();
            let mut a1 = RandomAgent { rng: thread_rng() };
            let mut a2 = RandomAgent { rng: thread_rng() };
            let mut rng = thread_rng();

            run_game(&mut g, &mut vec![&mut a1, &mut a2], &mut rng);

            let result = g.evaluate();
            score[0] += result[0];
            score[1] += result[1];
        }

        println!("{} vs {}", score[0], score[1])
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
