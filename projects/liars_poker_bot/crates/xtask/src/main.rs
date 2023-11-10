use std::path::Path;

use anyhow::Ok;
use clap::{command, Parser, Subcommand};
use itertools::Itertools;
use notify::{RecursiveMode, Watcher};

use xshell::{cmd, Shell};

const REMOTE_ADDR: &str = "static.222.71.9.5.clients.your-server.de";

#[derive(Debug, Subcommand, Clone)]
enum Commands {
    TrainLogs,
    ServerLogs,
    Serve,
    Deploy,
    UpdateNginx,
    Profile {
        #[clap(short, long, default_value_t = 0)]
        pid: usize,
    },
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
        Commands::TrainLogs => get_train_logs(),
        Commands::ServerLogs => get_server_logs(),
        Commands::Serve => serve(),
        Commands::Deploy => deploy(),
        Commands::UpdateNginx => update_nginx(),
        Commands::Profile { pid } => profile(pid),
    }
}

fn profile(pid: usize) -> anyhow::Result<()> {
    let sh = Shell::new()?;

    let kptr_restrict = cmd!(sh, "cat /proc/sys/kernel/kptr_restrict").read()?;
    if kptr_restrict != "0" {
        cmd!(sh, "sudo sh -c 'echo 0 > /proc/sys/kernel/kptr_restrict'").run()?;
    }
    let pid = if pid == 0 {
        cmd!(sh, "pidof card_platypus").read()?
    } else {
        pid.to_string()
    };
    cmd!(sh, "perf record -p {pid} -F 99 --call-graph dwarf sleep 60").run()?;

    Ok(())
}

fn get_train_logs() -> anyhow::Result<()> {
    let remote_log_file = "liars_poker.log";
    let local_log_file = "remote.log";

    let sh = Shell::new()?;
    cmd!(
        sh,
        "rsync root@{REMOTE_ADDR}:~/swpecht.github.io/projects/liars_poker_bot/{remote_log_file} {local_log_file}"
    )
    .run()?;

    cmd!(sh, "cat {local_log_file}").run()?;

    Ok(())
}

fn get_server_logs() -> anyhow::Result<()> {
    let local_log_file = "server.log";

    let sh = Shell::new()?;
    cmd!(
        sh,
        "rsync root@{REMOTE_ADDR}:~/deploy/euchre_server.log {local_log_file}"
    )
    .run()?;

    let logs = cmd!(sh, "cat {local_log_file}").read()?;

    let num_player_ids = logs
        .split('\n')
        .filter(|x| x.contains("player_id"))
        .filter_map(|x| x.split(' ').nth(7))
        .unique()
        .count();

    println!("unique player ids: {}", num_player_ids);

    let num_games = logs
        .split('\n')
        .filter(|x| x.contains("game ended"))
        .count();

    println!("num games: {}", num_games);

    //cat server.log | cut -c -10 | datamash -g 1 count 1

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

    watcher.watch(
        Path::new("./crates/euchre-app/index.html"),
        RecursiveMode::Recursive,
    )?;

    let sh = Shell::new()?;
    sh.change_dir("crates/euchre_server");
    cmd!(sh, "cargo watch --ignore euchre_server.log -x run").run()?;

    Ok(())
}

fn build_and_deploy_app() {
    let sh = Shell::new().unwrap();
    sh.change_dir("crates/euchre-app");

    let result = cmd!(sh, "dx build").run();
    if let Err(e) = result {
        println!("Error: {:?}", e);
    }

    cmd!(sh, "npx tailwindcss -i ./input.css -o ./dist/tailwind.css")
        .run()
        .unwrap();

    sh.change_dir("..");
    cmd!(sh, "rsync -r ./euchre-app/dist/. ./euchre_server/static")
        .run()
        .unwrap();
}

fn deploy() -> anyhow::Result<()> {
    let sh = Shell::new()?;
    sh.change_dir("crates/euchre-app");

    cmd!(sh, "dx build --profile wasm").run()?;

    cmd!(sh, "npx tailwindcss -i ./input.css -o ./dist/tailwind.css").run()?;

    cmd!(sh, "rsync -r ./dist/. root@{REMOTE_ADDR}:~/deploy/static").run()?;

    sh.change_dir("../euchre_server");

    cmd!(sh, "cargo build --release").run()?;
    cmd!(
        sh,
        "rsync ../../target/release/euchre_server root@{REMOTE_ADDR}:~/deploy"
    )
    .run()?;

    sh.change_dir("../xtask");
    cmd!(
        sh,
        "rsync euchre-server.service root@{REMOTE_ADDR}:/etc/systemd/system/"
    )
    .run()?;

    cmd!(sh, "ssh root@{REMOTE_ADDR} systemctl daemon-reload").run()?;

    cmd!(
        sh,
        "ssh root@{REMOTE_ADDR} systemctl restart eucher-server.service"
    )
    .run()?;

    Ok(())
}

fn update_nginx() -> anyhow::Result<()> {
    let sh = Shell::new()?;
    sh.change_dir("crates/xtask");
    cmd!(
        sh,
        "rsync nginx-default root@{REMOTE_ADDR}:/etc/nginx/sites-enabled/default"
    )
    .run()?;
    cmd!(sh, "ssh root@{REMOTE_ADDR} nginx -s reload").run()?;

    Ok(())
}
