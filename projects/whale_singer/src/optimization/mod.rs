use anyhow::{bail, Context};
use itertools::Itertools;
use sonogram::{ColourGradient, ColourTheme, SpecOptionsBuilder};

use crate::encode::SAMPLE_RATE;

use self::error::{dssim_error, rms_error};

pub mod error;

const MAX_ITERATIONS: usize = 20;

#[derive(Clone, Debug, Default)]
struct Samples(Vec<f32>);

#[derive(Clone, Debug, Default)]
pub(super) struct RBGA {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

pub enum AtomSearchResult {
    NoImprovement,
    Found {
        start: f64,
        atom_index: usize,
        old_error: f64,
        new_error: f64,
    },
}

/// Adds a single atom that results in the lowest error
pub fn add_best_atom(
    output: &mut [f32],
    target: &[f32],
    atoms: &[Vec<f32>],
) -> anyhow::Result<AtomSearchResult> {
    let target_spec = calculate_spectogram(Samples(target.to_vec()));
    let output_spec = calculate_spectogram(Samples(output.to_vec()));

    let old_error = dssim_error(&target_spec, &output_spec)?;

    let mut best_start = None;
    let mut best_error = old_error;

    let mut best_atom_index = 0;

    // let atom_spectograms = atoms
    //     .iter()
    //     .map(|x| calculate_spectogram(Samples(x.clone())))
    //     .collect_vec();

    for (atom_index, atom) in atoms.iter().enumerate() {
        for start in (0..target.len()).step_by(SAMPLE_RATE / 10) {
            add_atom(output, atom, start);
            let output_spec = calculate_spectogram(Samples(output.to_vec()));
            let error =
                dssim_error(&target_spec, &output_spec).context("failed to calculate error")?;
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
                old_error,
                new_error: best_error,
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

fn calculate_spectogram(samples: Samples) -> RBGA {
    let mut spectrograph = SpecOptionsBuilder::new(1024)
        .load_data_from_memory_f32(samples.0.clone(), SAMPLE_RATE as u32)
        .build()
        .unwrap();
    // Compute the spectrogram giving the number of bins and the window overlap.
    let mut gradient = ColourGradient::create(ColourTheme::Default);
    let mut spectrograph = spectrograph.compute();
    let buf =
        spectrograph.to_rgba_in_memory(sonogram::FrequencyScale::Linear, &mut gradient, 512, 512);
    RBGA {
        width: 512,
        height: 512,
        data: buf,
    }
}
