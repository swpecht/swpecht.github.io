use std::{collections::HashSet, fs::File, io::BufWriter, path::Path};

use anyhow::{bail, Ok};
use boomphf::Mphf;
use card_platypus::{database::euchre_states::collect_istates, io::ProgressReader};
use clap::Subcommand;
use games::{
    gamestates::euchre::actions::{Card, EAction},
    istate::IStateKey,
    Action,
};
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
    let r = ProgressReader::new(&out_dir.join(ISTATE_FILE))?;
    let istates = match serde_json::from_reader(r) {
        std::result::Result::Ok(x) => x,
        Err(_) => HashSet::default(),
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

use EAction::*;
const VALID_ACTIONS: &[&[EAction]] = &[
    &[
        NC, TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD,
    ],
    &[
        TC, JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD,
    ],
    &[
        JC, QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD, QD,
    ],
    &[
        QC, KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD, QD, KD,
    ],
    &[
        KC, AC, NS, TS, JS, QS, KS, AS, NH, TH, JH, QH, KH, AH, ND, TD, JD, QD, KD, AD,
    ],
    &[NS, TS, JS, QS, KS, AS],
];

/// Translates an IStateKey to an index
pub fn to_index(key: &IStateKey) -> anyhow::Result<usize> {
    if key.len() < 6 {
        bail!("only full deals supported")
    }

    assert_eq!(key.len(), 6, "only support deals for now");

    let mut index = 0;
    let mut actions = Vec::with_capacity(key.len());
    for a in key.iter().map(|x| EAction::from(*x)) {
        actions.push(a);
        index += count_lower(&mut actions)?;
    }

    Ok(index)
}

/// Returns the number of lower hands if the deal was extended forward from the
/// last dealt card where it was filled with the lowest cards
fn count_lower(actions: &mut Vec<EAction>) -> anyhow::Result<usize> {
    if actions.len() == 6 {
        return Ok(1);
    }

    // we should count up, until we have the action we want in the slot we're looking at?
    let mut count = 0;
    let valid = VALID_ACTIONS[actions.len() - 1];
    let a = actions.last().unwrap();
    // let idx = valid.iter().position(|x| *x == a).with_context(|| {
    //     format!(
    //         "invalid action passsed: {}, valid actions are: {:?}",
    //         a, valid
    //     )
    // })?;
    let idx = 0;
    let lower_actions = &valid[..idx];

    for a in lower_actions {
        actions.push(*a);
        count += count_lower(actions)?;
        actions.pop();
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use anyhow::Ok;

    use super::VALID_ACTIONS;

    // #[test]
    // fn test_valid_actions_sorted() {
    //     for list in VALID_ACTIONS {
    //         let mut sorted = list.to_vec();
    //         sorted.sort();
    //         assert_eq!(list.to_vec(), sorted);
    //     }
    // }

    // #[test]
    // fn test_euchre_index() -> anyhow::Result<()> {
    //     use Card::*;
    //     let cases = vec![
    //         (vec![NC, TC, JC, QC, KC, NS], 0),
    //         (vec![NC, TC, JC, QC, KC, TS], 1),
    //         (vec![TC, JC, QC, KC, AC, NS], 31),
    //         (vec![JC, QC, KC, AC, NS, NS], 242),
    //         (vec![TD, JD, QD, KD, AD, AS], 168245),
    //     ];

    //     for (cards, index) in cases {
    //         let key = to_key(&cards);
    //         assert_eq!(to_index(&key)?, index, "{:?}", cards);
    //     }

    //     Ok(())
    // }

    // fn to_key(cards: &[Card]) -> IStateKey {
    //     let mut key = IStateKey::default();
    //     for c in cards {
    //         key.push(EAction::from(*c).into());
    //     }

    //     key
    // }
}
