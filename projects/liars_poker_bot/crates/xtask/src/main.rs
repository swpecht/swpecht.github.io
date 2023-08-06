use std::{path::Path, thread};

use anyhow::Ok;
use clap::{command, Parser, Subcommand};
use notify::{RecursiveMode, Watcher};
use xshell::{cmd, Shell};

const REMOTE_ADDR: &str = "static.222.71.9.5.clients.your-server.de";

#[derive(Debug, Subcommand, Clone)]
enum Commands {
    RemoteLogs,
    Serve,
    Deploy,
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
        Commands::Deploy => deploy(),
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
    // Automatically select the best implementation for your platform.
    let mut watcher =
        notify::recommended_watcher(|res: Result<notify::Event, notify::Error>| match res {
            std::result::Result::Ok(event) => {
                if event
                    .paths
                    .iter()
                    .any(|x| x.extension().map_or(false, |x| x == "html" || x == "rs"))
                {
                    println!("{:?}", event);
                    build_and_deploy_app()
                }
            }
            Err(e) => println!("watch error: {:?}", e),
        })?;

    build_and_deploy_app();
    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(
        Path::new("./crates/euchre-app/src"),
        RecursiveMode::Recursive,
    )?;
    // watcher.watch(
    //     Path::new("./crates/euchre-app/dist"),
    //     RecursiveMode::Recursive,
    // )?;

    let sh = Shell::new()?;
    sh.change_dir("crates/euchre_server");
    cmd!(sh, "cargo watch --ignore euchre_server.log -x run").run()?;

    Ok(())
}

fn build_and_deploy_app() {
    let sh = Shell::new().unwrap();
    sh.change_dir("crates/euchre-app");

    let result = cmd!(sh, "dx build --release").run();
    if let Err(e) = result {
        println!("Error: {:?}", e);
    }

    cmd!(
        sh,
        "npx tailwindcss -i ./input.css -o ./public/tailwind.css"
    )
    .run()
    .unwrap();

    sh.change_dir("..");
    cmd!(sh, "cp -r ./euchre-app/dist/. ./euchre_server/static")
        .run()
        .unwrap();
}

fn deploy() -> anyhow::Result<()> {
    let sh = Shell::new()?;
    sh.change_dir("crates/euchre-app");
    cmd!(sh, "dx build --release").run()?;

    cmd!(sh, "scp -r ./dist/. root@{REMOTE_ADDR}:~/deploy/static").run()?;

    sh.change_dir("../euchre_server");

    cmd!(sh, "cargo build --release").run()?;
    cmd!(
        sh,
        "scp ../../target/release/euchre_server root@{REMOTE_ADDR}:~/deploy"
    )
    .run()?;

    Ok(())
}
