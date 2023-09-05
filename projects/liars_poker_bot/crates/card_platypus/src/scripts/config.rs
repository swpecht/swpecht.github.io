use std::{collections::HashMap, fs};

use anyhow::Context;
use log::info;
use serde::Deserialize;

use super::pass_on_bower_cfr::{run_pass_on_bower_cfr, PassOnBowerCFRArgs};

const CONFIG_LOCATION: &str = "./Train.toml";

#[derive(Deserialize, Debug)]
struct Config {
    train: HashMap<String, PassOnBowerCFRArgs>,
}

pub fn train_cfr_from_config(profile: &str) -> anyhow::Result<()> {
    info!("starting config: {}", profile);
    let toml_str = fs::read_to_string(CONFIG_LOCATION)?;
    let toml: Config = toml::from_str(&toml_str)?;

    let args = toml.train.get(profile).context("profile not found")?;
    run_pass_on_bower_cfr(args.clone());

    Ok(())
}
