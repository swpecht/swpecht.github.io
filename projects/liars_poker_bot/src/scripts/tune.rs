use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum TuneMode {
    Compare,
    ParameterSearch,
}

#[derive(Debug, ValueEnum, Clone, Copy)]
pub enum AgentAlgorithm {
    AlphaMu,
    PIMCTS,
}

#[derive(Debug, Args, Clone, Copy)]
pub struct TuneArgs {
    #[clap(long, short = 'n', default_value_t = 1)]
    num_games: usize,
    #[clap(long, value_enum)]
    algorithm: AgentAlgorithm,
}

pub fn run_tune(args: TuneMode) {}

fn compare_agents(args: TuneArgs) {}
