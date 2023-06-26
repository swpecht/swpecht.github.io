use clap::{Args, Subcommand, ValueEnum};
use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        ismcts::{ChildSelectionPolicy, ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType},
        open_hand_solver::OpenHandSolver,
        pimcts::{self, PIMCTSBot},
    },
    game::{euchre::EuchreGameState, GameState},
};
use log::info;
use rand::SeedableRng;

use crate::scripts::{benchmark::rng, pass_on_bower_alpha::get_bower_deals};

#[derive(Debug, Subcommand, Clone, Copy)]
pub enum TuneMode {
    Compare {
        test_algorithm: AgentAlgorithm,
        #[clap(short, long, default_value_t = 20)]
        worlds: usize,
        #[clap(short, long, default_value_t = 3)]
        m: usize,
        num_games: usize,
    },
    ParameterSearch {
        num_games: usize,
        #[clap(long, value_enum)]
        algorithm: AgentAlgorithm,
    },
}

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum AgentAlgorithm {
    AlphaMu,
    PIMCTS,
    ISMCTS,
}

#[derive(Debug, Args, Clone, Copy)]
pub struct TuneArgs {
    #[command(subcommand)]
    command: TuneMode,
}

pub fn run_tune(args: TuneArgs) {
    match args.command {
        TuneMode::ParameterSearch {
            num_games,
            algorithm,
        } => {
            println!("starting tune rune for: {:?}, n={}", algorithm, num_games);
            match algorithm {
                AgentAlgorithm::AlphaMu => tune_alpha_mu(num_games),
                AgentAlgorithm::PIMCTS => tune_pimcts(num_games),
                AgentAlgorithm::ISMCTS => tune_ismcts(num_games),
            }
        }
        TuneMode::Compare {
            test_algorithm,
            worlds,
            m,
            num_games,
        } => todo!(),
    }
}

/// Compare alpha mu performance for different world sizes and m on deals where
/// the dealer has a face up jack
fn tune_alpha_mu(num_games: usize) {
    info!("m\tnum worlds\tavg score");

    let ms = vec![1, 2, 3, 5];
    let world_counts = vec![5, 10, 15, 20];
    let worlds = get_bower_deals(num_games, &mut rng());

    for m in ms {
        for count in world_counts.clone() {
            let alphamu = PolicyAgent::new(
                AlphaMuBot::new(OpenHandSolver::new(), count, m, rng()),
                rng(),
            );
            let returns = get_returns(alphamu, worlds.clone());
            info!("{}\t{}\t{:?}", m, count, returns / num_games as f64);
        }
    }
}

fn tune_ismcts(num_games: usize) {
    info!("fiinal_policy\tselection\tuct_c\tmax_simulations\tavg score");

    let uct_values = vec![0.001, 0.1, 0.5, 1.0, 3.0, 5.0];
    let simulation_counts = vec![5, 10, 15, 20, 50, 100];
    let policy_types = vec![
        ISMCTSFinalPolicyType::MaxVisitCount,
        ISMCTSFinalPolicyType::NormalizedVisitedCount,
        ISMCTSFinalPolicyType::MaxValue,
    ];
    let child_selection_types = vec![ChildSelectionPolicy::Uct, ChildSelectionPolicy::Puct];
    let worlds = get_bower_deals(num_games, &mut rng());

    for p in policy_types {
        for c in child_selection_types.clone() {
            for uct_c in uct_values.clone() {
                for count in simulation_counts.clone() {
                    let config = ISMCTBotConfig {
                        child_selection_policy: c.clone(),
                        final_policy_type: p.clone(),
                        max_world_samples: -1,
                    };
                    let alphamu = PolicyAgent::new(
                        ISMCTSBot::new(uct_c, count, OpenHandSolver::new(), config),
                        rng(),
                    );
                    let returns = get_returns(alphamu, worlds.clone());
                    info!(
                        "{:?}\t{:?}\t{}\t{}\t{:?}",
                        p,
                        c,
                        uct_c,
                        count,
                        returns / num_games as f64
                    );
                }
            }
        }
    }
}

fn tune_pimcts(num_games: usize) {
    info!("num worlds\tavg score");
    let world_counts = vec![5, 10, 15, 20, 50, 100, 200];
    let worlds = get_bower_deals(num_games, &mut rng());

    for count in world_counts {
        let pimcts = PolicyAgent::new(PIMCTSBot::new(count, OpenHandSolver::new(), rng()), rng());
        let returns = get_returns(pimcts, worlds.clone());
        info!("{}\t{:?}", count, returns / num_games as f64);
    }
}

fn get_returns<T: Agent<EuchreGameState>>(mut test_agent: T, worlds: Vec<EuchreGameState>) -> f64 {
    // Opponent always starts with same seed
    let opponent = &mut get_opponent();
    let mut returns = 0.0;

    // all agents play the same games
    for gs in worlds.clone().iter_mut() {
        while !gs.is_terminal() {
            // Alphamu is the dealer team
            let a = if gs.cur_player() % 2 == 1 {
                test_agent.step(gs)
            } else {
                opponent.step(gs)
            };
            gs.apply_action(a);
        }
        // get the returns for alpha mu's team
        returns += gs.evaluate(1);
    }

    returns
}

fn get_opponent() -> PolicyAgent<PIMCTSBot<EuchreGameState, OpenHandSolver>> {
    PolicyAgent::new(
        PIMCTSBot::new(50, OpenHandSolver::new(), SeedableRng::seed_from_u64(100)),
        SeedableRng::seed_from_u64(101),
    )
}

fn compare_agents(args: TuneArgs) {}
