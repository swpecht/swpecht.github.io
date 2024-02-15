use anyhow::{bail, Context};
use itertools::Itertools;
use sonogram::SpecOptionsBuilder;

use crate::encode::SAMPLE_RATE;

use self::error::rms_error;

pub mod error;

const MAX_ITERATIONS: usize = 20;

#[derive(Clone, Debug, Default)]
struct Samples(Vec<f32>);

#[derive(Clone, Debug, Default)]
struct Spectogram(Vec<f32>);

pub enum AtomSearchResult {
    NoImprovement,
    Found { start: f64, atom_index: usize },
}

pub fn find_best_match(target: &[f32], atoms: Vec<Vec<f32>>) -> anyhow::Result<Vec<f32>> {
    let mut output = vec![0.0; target.len()];

    for _ in 0..MAX_ITERATIONS {
        match add_best_atom(&mut output, target, &atoms)? {
            AtomSearchResult::NoImprovement => break,
            AtomSearchResult::Found { start, atom_index } => println!("{}: {}s", atom_index, start),
        }
    }

    Ok(output)
}

/// Adds a single atom that results in the lowest error
pub fn add_best_atom(
    output: &mut [f32],
    target: &[f32],
    atoms: &[Vec<f32>],
) -> anyhow::Result<AtomSearchResult> {
    let mut best_start = None;
    let mut best_error = rms_error(target, output)?;
    let mut best_atom_index = 0;

    let atom_spectograms = atoms
        .iter()
        .map(|x| calculate_spectogram(Samples(x.clone())))
        .collect_vec();

    let target_spec = calculate_spectogram(Samples(target.to_vec()));

    for (atom_index, atom) in atoms.iter().enumerate() {
        for start in (0..target.len()).step_by(SAMPLE_RATE / 10) {
            add_atom(output, atom, start);
            let output_spec = calculate_spectogram(Samples(output.to_vec()));
            let error =
                rms_error(&target_spec.0, &output_spec.0).context("failed to calculate error")?;
            if error < best_error {
                best_error = error;
                best_start = Some(start);
                best_atom_index = atom_index;
            }
            subtract_atom(output, atom, start);
        }
    }

    Ok(match best_start {
        None => AtomSearchResult::NoImprovement,
        Some(start) => {
            add_atom(output, &atoms[best_atom_index], start);
            AtomSearchResult::Found {
                start: start as f64 / SAMPLE_RATE as f64,
                atom_index: best_atom_index,
            }
        }
    })
}

fn add_atom(output: &mut [f32], atom: &[f32], start: usize) {
    output
        .iter_mut()
        .skip(start)
        .zip(atom.iter())
        .for_each(|(o, a)| *o += a);
}

fn subtract_atom(output: &mut [f32], atom: &[f32], start: usize) {
    output
        .iter_mut()
        .skip(start)
        .zip(atom.iter())
        .for_each(|(o, a)| *o -= a);
}

fn calculate_spectogram(samples: Samples) -> Spectogram {
    let mut spectrograph = SpecOptionsBuilder::new(1024)
        .load_data_from_memory_f32(samples.0.clone(), SAMPLE_RATE as u32)
        .build()
        .unwrap();
    // Compute the spectrogram giving the number of bins and the window overlap.
    let spectrograph = spectrograph.compute();
    let buf = spectrograph.to_buffer(sonogram::FrequencyScale::Linear, 512, 512);
    Spectogram(buf)
}
