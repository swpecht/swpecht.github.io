use std::{
    collections::HashSet,
    fs::File,
    io::{BufReader, BufWriter},
    path::Path,
};

use card_platypus::{database::euchre_states::collect_istates, game::euchre::actions::Card};
use clap::Subcommand;
use indicatif::ProgressBar;
use log::info;

#[derive(Subcommand, Copy, Clone, Debug)]
pub enum EuchrePhfMode {
    GenerateIstates { num_iterations: usize },
    GeneratePhf,
}

pub fn euchre_phf(command: EuchrePhfMode) {
    match command {
        EuchrePhfMode::GenerateIstates { num_iterations } => {
            generate_euchre_istates(num_iterations).unwrap()
        }
        EuchrePhfMode::GeneratePhf => todo!(),
    }
}

fn generate_euchre_istates(num_iterations: usize) -> anyhow::Result<()> {
    println!("loading previous istates");
    let out_dir = Path::new("/var/lib/card_platypus");
    std::fs::create_dir_all(out_dir)?;
    let mut istates = if let Ok(out_f) = File::open(out_dir.join("euchre_istates")) {
        let r = BufReader::new(&out_f);
        match serde_json::from_reader(r) {
            Ok(x) => x,
            Err(_) => HashSet::default(),
        }
    } else {
        HashSet::default()
    };
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

    let out_f = File::create(out_dir.join("euchre_istates"))?;
    let w = BufWriter::new(out_f);
    serde_json::to_writer(w, &istates)?;

    println!("generated {} istates", istates.len());

    Ok(())
}
