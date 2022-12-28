pub mod agents;
pub mod game;
pub mod kuhn_poker;
pub mod qagent;

use clap::Parser;

use clap::clap_derive::ArgEnum;

use game::{run_game, GameState};
use kuhn_poker::KuhnPoker;
use qagent::QAgent;
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
        todo!()
    } else {
        let mut score = [0.0; 2];
        let mut a1 = QAgent::new(0);
        print!("{:?}", a1.get_weight());
        let mut a2 = QAgent::new(1);
        let mut rng = thread_rng();
        for _ in 0..args.num_games {
            let mut g = KuhnPoker::new();
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
