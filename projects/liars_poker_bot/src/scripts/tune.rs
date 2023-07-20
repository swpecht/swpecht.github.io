use clap::{Args, ValueEnum};
use indicatif::ProgressBar;
use itertools::Itertools;
use liars_poker_bot::{
    agents::{Agent, PolicyAgent},
    algorithms::{
        alphamu::AlphaMuBot,
        ismcts::{ChildSelectionPolicy, ISMCTBotConfig, ISMCTSBot, ISMCTSFinalPolicyType},
        open_hand_solver::OpenHandSolver,
        pimcts::PIMCTSBot,
    },
    cfragent::cfres::CFRES,
    game::{
        euchre::{actions::EAction, processors::post_discard_phase, Euchre, EuchreGameState},
        get_games, GameState,
    },
};
use log::{info, warn};
use rand::SeedableRng;

use crate::scripts::{benchmark::get_rng, pass_on_bower_alpha::get_bower_deals};

use super::pass_on_bower_cfr::generate_jack_of_spades_deal;

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum TuneMode {
    Compare,
    ParameterSearch,
}

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum AgentAlgorithm {
    AlphaMu,
    PIMCTS,
    ISMCTS,
}

#[derive(Debug, Args, Clone, Copy)]
pub struct TuneArgs {
    command: TuneMode,
    algorithm: AgentAlgorithm,
    #[clap(short, long, default_value_t = 20)]
    num_games: usize,
    #[clap(short, long, default_value_t = 20)]
    worlds: usize,
    #[clap(short, long, default_value_t = 3)]
    m: usize,
}

pub fn run_tune(args: TuneArgs) {
    match args.command {
        TuneMode::ParameterSearch => {
            println!(
                "starting tune rune for: {:?}, n={}",
                args.algorithm, args.num_games
            );
            match args.algorithm {
                AgentAlgorithm::AlphaMu => tune_alpha_mu(args.num_games),
                AgentAlgorithm::PIMCTS => tune_pimcts(args.num_games),
                AgentAlgorithm::ISMCTS => tune_ismcts(args.num_games),
            }
        }
        TuneMode::Compare => compare_agents(args),
    }
}

/// Compare alpha mu performance for different world sizes and m on deals where
/// the dealer has a face up jack
fn tune_alpha_mu(num_games: usize) {
    info!("m\tnum worlds\tavg score");

    let ms = vec![1, 5, 10, 20];
    let world_counts = vec![8, 16, 32];
    // let worlds = get_bower_deals(num_games, &mut get_rng());
    let worlds = get_games(Euchre::game(), num_games, &mut get_rng());

    for m in ms {
        for count in world_counts.clone() {
            let alphamu = PolicyAgent::new(
                AlphaMuBot::new(OpenHandSolver::new_euchre(), count, m, get_rng()),
                get_rng(),
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
    // let worlds = get_bower_deals(num_games, &mut get_rng());
    let worlds = get_games(Euchre::game(), num_games, &mut get_rng());

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
                        ISMCTSBot::new(uct_c, count, OpenHandSolver::new_euchre(), config),
                        get_rng(),
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
    let worlds = get_bower_deals(num_games, &mut get_rng());

    for count in world_counts {
        let pimcts = PolicyAgent::new(
            PIMCTSBot::new(count, OpenHandSolver::new_euchre(), get_rng()),
            get_rng(),
        );
        let returns = get_returns(pimcts, worlds.clone());
        info!("{}\t{:?}", count, returns / num_games as f64);
    }
}

fn get_returns<T: Agent<EuchreGameState>>(mut test_agent: T, worlds: Vec<EuchreGameState>) -> f64 {
    // Opponent always starts with same seed
    let opponent = &mut get_opponent();
    let mut returns = 0.0;

    let pb = ProgressBar::new(worlds.len() as u64);

    // all agents play the same games
    for mut gs in worlds.into_iter() {
        while !gs.is_terminal() {
            // Alphamu is the dealer team
            let a = if gs.cur_player() % 2 == 1 {
                test_agent.step(&gs)
            } else {
                opponent.step(&gs)
            };
            gs.apply_action(a);
        }
        // get the returns for alpha mu's team
        returns += gs.evaluate(1);
        pb.inc(1);
    }

    pb.finish_and_clear();
    returns
}

fn get_opponent() -> PolicyAgent<PIMCTSBot<EuchreGameState, OpenHandSolver<EuchreGameState>>> {
    PolicyAgent::new(
        PIMCTSBot::new(
            50,
            OpenHandSolver::new_euchre(),
            SeedableRng::seed_from_u64(100),
        ),
        SeedableRng::seed_from_u64(101),
    )
}

fn compare_agents(args: TuneArgs) {
    // let games = get_games(Euchre::game(), args.num_games, &mut get_rng());

    warn!("only using jack of spades deals");
    let games = (0..args.num_games)
        .map(|_| generate_jack_of_spades_deal())
        .collect_vec();

    let mut pimcts = get_opponent();
    // Based on tuning run for 100 games
    // https://docs.google.com/spreadsheets/d/1AGjEaqjCkuuWveUBqbOBOMH0SPHPQ_YhH1jRHij7ErY/edit#gid=1418816031
    // let config = ISMCTBotConfig {
    //     child_selection_policy: ChildSelectionPolicy::Uct,
    //     final_policy_type: ISMCTSFinalPolicyType::MaxVisitCount,
    //     max_world_samples: -1, // unlimited samples
    // };
    // let mut test_agent = PolicyAgent::new(
    //     ISMCTSBot::new(3.0, 100, OpenHandSolver::new(), config),
    //     rng(),
    // );

    let mut test_agent = CFRES::new(Euchre::new_state, get_rng());
    test_agent.load("infostates");

    let mut test_agent = PolicyAgent::new(test_agent, get_rng());

    for mut gs in games {
        while !post_discard_phase(&gs) {
            let baseline_a = pimcts.step(&gs);
            let test_a = test_agent.step(&gs);

            if baseline_a != test_a {
                info!(
                    "{}: {}: baseline: {}, test: {}",
                    gs.cur_player(),
                    gs,
                    EAction::from(baseline_a),
                    EAction::from(test_a)
                );
            }

            gs.apply_action(baseline_a);
        }
    }
}
