use anyhow::Ok;
use clap::{command, Parser, Subcommand};
use xshell::{cmd, Shell};

const REMOTE_ADDR: &str = "static.222.71.9.5.clients.your-server.de";

#[derive(Debug, Subcommand, Clone)]
enum Commands {
    RemoteLogs,
    Serve,
}

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::RemoteLogs => get_remote_logs(),
        Commands::Serve => serve(),
    }
}

fn get_remote_logs() -> anyhow::Result<()> {
    let remote_log_file = "liars_poker.log";
    let local_log_file = "remote.log";

    let sh = Shell::new()?;
    cmd!(
        sh,
        "scp root@{REMOTE_ADDR}:~/swpecht.github.io/projects/liars_poker_bot/{remote_log_file} {local_log_file}"
    )
    .run()?;

    cmd!(sh, "cat {local_log_file}").run()?;

    Ok(())
}

fn serve() -> anyhow::Result<()> {
    let sh = Shell::new()?;
    sh.change_dir("crates/euchre-app");
    cmd!(sh, "dioxus serve").run()?;

    Ok(())
}
