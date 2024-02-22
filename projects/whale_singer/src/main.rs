use std::env::{self};

use log::info;
use whale_singer::{
    app::run_app,
    decode::extract_samples,
    encode::{save_wav, SAMPLE_RATE},
    optimization::{AtomOptimizer, AtomSearchResult},
};

use color_eyre::Result;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // just the binary name
    if args.len() == 1 {
        return run_app();
    } else if args[1] == "scratch" {
        run_scratch();
        return color_eyre::Result::Ok(());
    }

    panic!("invalid arguments: {:?}", args);
}

fn run_scratch() {
    let src = std::fs::File::open("im_different_sample.wav").expect("failed to open media");
    // let src = std::fs::File::open("happy_birthday.mp3").expect("failed to open media");
    let mut target_samples = extract_samples(src).unwrap();
    target_samples.truncate(SAMPLE_RATE * 4);

    let target = target_samples;

    // let paths = fs::read_dir("../../../piano-mp3/piano-mp3/").unwrap();

    let atom_files = ["A4", "B4", "C4", "D4", "E4", "F4", "G4"];
    let mut atoms = Vec::new();
    for atom_id in atom_files {
        let path_name = format!("../../../piano-mp3/piano-mp3/{}.mp3", atom_id);
        // let src =
        //     std::fs::File::open(atom_file.unwrap().path()).expect("failed to open media");
        let src = std::fs::File::open(path_name).expect("failed to open media");
        let key_samples = extract_samples(src).unwrap();
        // only the the middle 1 second
        // key_samples = key_samples[SAMPLE_RATE * 3 / 4..SAMPLE_RATE * 5 / 4].to_vec();
        atoms.push(key_samples);
    }

    info!("loaded {} atoms", atoms.len());

    let mut atom_finder = AtomOptimizer::new(&target, &atoms);

    loop {
        match atom_finder.add_best_chunk().unwrap() {
            AtomSearchResult::NoImprovement => {
                info!("failed to find improvement");
                break;
            }
            AtomSearchResult::Found { details } => {
                let samples: Vec<f32> = atom_finder.cur_samples().into();
                save_wav("output.wav", &samples).unwrap();
                info!(
                    "found improvement with atom: {} at start: {}, old error: {}, new error: {}",
                    details.atom_index,
                    details.chunk,
                    details.chunk_old_error,
                    details.chunk_new_error
                );
            }
        }
    }

    info!("finished searching");
}
