use std::{
    collections::HashSet,
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
};

use boomphf::Mphf;
use card_platypus::{
    database::euchre_states::collect_istates,
    game::{euchre::actions::Card, Action},
};
use clap::Subcommand;
use indicatif::ProgressBar;
use itertools::Itertools;
use log::info;

const DIR: &str = "/var/lib/card_platypus";
const ISTATE_FILE: &str = "euchre_istates";
const PHF_FILE: &str = "euchre_phf";

#[derive(Subcommand, Copy, Clone, Debug)]
pub enum EuchrePhfMode {
    GenerateIstates { num_iterations: usize },
    GeneratePhf,
}

pub fn euchre_phf(command: EuchrePhfMode) {
    let out_dir = Path::new(DIR);
    std::fs::create_dir_all(out_dir).unwrap();

    match command {
        EuchrePhfMode::GenerateIstates { num_iterations } => {
            generate_euchre_istates(num_iterations).unwrap()
        }
        EuchrePhfMode::GeneratePhf => generate_euchre_phf().unwrap(),
    }
}

fn generate_euchre_istates(num_iterations: usize) -> anyhow::Result<()> {
    println!("loading previous istates");
    let mut istates = load_istates()?;
    println!("loaded {} istates", istates.len());

    let mut cur_sample = 0;
    const STEP_SIZE: usize = 1_000_000;

    let pb = ProgressBar::new(num_iterations as u64);
    while cur_sample < num_iterations {
        collect_istates(&mut istates, STEP_SIZE, Card::NS, 4);
        cur_sample += STEP_SIZE;
        pb.inc(STEP_SIZE as u64);
        info!("step:\t{}\tistates:\t{}", cur_sample, istates.len());
    }
    pb.finish_and_clear();

    let out_dir = Path::new(DIR);
    let out_f = File::create(out_dir.join(ISTATE_FILE))?;
    let w = BufWriter::new(out_f);
    serde_json::to_writer(w, &istates)?;

    println!("generated {} istates", istates.len());

    Ok(())
}

fn load_istates() -> anyhow::Result<HashSet<Vec<Action>>> {
    let out_dir = Path::new(DIR);
    let istates = if let Ok(out_f) = File::open(out_dir.join(ISTATE_FILE)) {
        let r = BufReader::new(&out_f);
        match serde_json::from_reader(r) {
            Ok(x) => x,
            Err(_) => HashSet::default(),
        }
    } else {
        HashSet::default()
    };
    Ok(istates)
}

fn generate_euchre_phf() -> anyhow::Result<()> {
    println!("loading previous istates");
    let mut istates = load_istates()?;
    println!("loaded {} istates", istates.len());

    let phf = Mphf::new_parallel(1.7, &istates.drain().collect_vec(), None);

    let out_dir = Path::new(DIR);
    let out_f = File::create(out_dir.join(PHF_FILE))?;
    let w = BufWriter::new(out_f);
    serde_json::to_writer(w, &phf)?;

    Ok(())
}
