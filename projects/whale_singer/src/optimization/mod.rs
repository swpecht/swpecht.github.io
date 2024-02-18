use anyhow::{bail, Context};

use crate::encode::SAMPLE_RATE;

use self::error::{rms_error, weighted_error};

pub mod error;

const MAX_ITERATIONS: usize = 20;

type ErrorFn = fn(&[f32], &[f32]) -> anyhow::Result<f64>;

/// Find the next best atom
pub struct AtomOptimizer {
    /// Number of samples in a chunk
    chunk_len: usize,
    sample_rate: usize,
    atoms: Vec<Vec<f32>>,
    error_fn: ErrorFn,
}

#[derive(Clone, Debug, Default)]
struct Samples(Vec<f32>);

pub enum AtomSearchResult {
    NoImprovement,
    Found {
        start: f64,
        atom_index: usize,
        old_error: f64,
        new_error: f64,
    },
}

impl Default for AtomOptimizer {
    fn default() -> Self {
        Self {
            chunk_len: SAMPLE_RATE / 10,
            sample_rate: SAMPLE_RATE,
            atoms: Vec::new(),
            error_fn: weighted_error,
        }
    }
}

impl AtomOptimizer {
    /// Adds a chunk from the atoms that most reduces the error between output and target
    pub fn add_best_chunk(
        &mut self,
        output: &mut [f32],
        target: &[f32],
    ) -> anyhow::Result<AtomSearchResult> {
        if self.atoms.is_empty() {
            bail!("tried to call optimizer with no atoms");
        }
        let old_error = (self.error_fn)(output, target)?;

        let mut best_start = None;
        let mut best_error = f64::INFINITY; // old_error;

        let mut best_atom_index = 0;

        for (atom_index, atom) in self.atoms.iter().enumerate() {
            for start in (0..target.len()).step_by(self.chunk_len) {
                add_atom(output, atom, start);
                let error = (self.error_fn)(target, output).context("failed to calculate error")?;
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
                add_atom(output, &self.atoms[best_atom_index], start);
                AtomSearchResult::Found {
                    start: start as f64 / self.sample_rate as f64,
                    atom_index: best_atom_index,
                    old_error,
                    new_error: best_error,
                }
            }
        })
    }

    pub fn set_atoms(&mut self, atoms: Vec<Vec<f32>>) {
        self.atoms = atoms;
    }
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
