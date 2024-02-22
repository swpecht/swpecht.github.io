use anyhow::{Context, Ok};

use log::debug;

use crate::{app::populate_progress, encode::SAMPLE_RATE, samples::Samples};

use self::{
    chunks::{to_chunks, Chunk, ConstructedSample},
    error::weighted_error,
};

mod chunks;
pub mod error;

type ErrorFn = fn(&Samples, &Samples) -> anyhow::Result<f64>;

/// Find the next best atom
pub struct AtomOptimizer {
    /// Number of samples in a chunk
    chunk_len: usize,
    sample_rate: usize,
    error_fn: ErrorFn,
    target_chunks: Vec<Chunk>,
    atom_chunks: Vec<Chunk>,
    candidates: Vec<Option<AtomSearchResult>>,
    constructed_sample: ConstructedSample,
}

#[derive(Debug, Clone)]
pub enum AtomSearchResult {
    NoImprovement,
    Found { details: ImprovementDetails },
}

#[derive(Debug, Clone)]
pub struct ImprovementDetails {
    pub chunk: usize,
    pub atom_index: usize,
    pub atom_chunk: Chunk,
    pub chunk_old_error: f64,
    pub chunk_new_error: f64,
}

impl ImprovementDetails {
    pub fn improvement(&self) -> f64 {
        self.chunk_old_error - self.chunk_new_error
    }
}

impl AtomOptimizer {
    pub fn new(target: &[f32], atoms: &[Vec<f32>]) -> Self {
        let chunk_len = SAMPLE_RATE / 1000;

        let sample_chunks = to_chunks(&[target.to_vec()], chunk_len);
        let atom_chunks = to_chunks(atoms, chunk_len);
        debug!("converted atoms into {} atom chunks", atom_chunks.len());

        Self {
            chunk_len,
            sample_rate: SAMPLE_RATE,
            error_fn: weighted_error,
            candidates: vec![None; sample_chunks.len()],
            constructed_sample: ConstructedSample::new(chunk_len, sample_chunks.len()),
            target_chunks: sample_chunks,
            atom_chunks,
        }
    }

    pub fn cur_samples(&self) -> Samples {
        self.constructed_sample.samples()
    }

    /// Adds a chunk from the atoms that most reduces the error between output and target
    pub fn add_best_chunk(&mut self) -> anyhow::Result<AtomSearchResult> {
        self.populate_candidates()?;

        let mut best_candidate_details = None;
        let mut best_improvement = f64::NEG_INFINITY;
        for candidate in self.candidates.iter() {
            use AtomSearchResult::*;
            match candidate {
                Some(Found { details }) => {
                    if details.improvement() > best_improvement {
                        best_candidate_details = Some(details.clone());
                        best_improvement = details.improvement();
                    }
                }
                Some(NoImprovement) => {}
                None => {}
            }
        }

        if let Some(details) = best_candidate_details {
            self.candidates[details.chunk] = None;
            self.constructed_sample
                .add_atom(details.chunk, details.atom_chunk.clone());
            Ok(AtomSearchResult::Found { details })
        } else {
            Ok(AtomSearchResult::NoImprovement)
        }
    }

    /// Populate all candidates with the atom_chunk that improves that single chunk the most
    fn populate_candidates(&mut self) -> anyhow::Result<()> {
        debug!("updating candidates");
        let mut new_candidates_found = 0;

        for (t_id, t_chunk) in self.target_chunks.iter().enumerate() {
            if self.candidates[t_id].is_some() {
                // don't re-calculate if we already know the best option
                continue;
            }
            populate_progress::set(t_id * 100 / self.target_chunks.len());

            let mut buffer = self.constructed_sample.chunk_samples(t_id);
            let sample_len = t_chunk.samples.len();
            buffer.truncate(sample_len);

            let old_error = (self.error_fn)(&t_chunk.samples, &buffer)?;
            let mut best_error = old_error;
            let mut best_atom_chunk_index = None;

            for atom in self.atom_chunks.iter() {
                buffer.add(&atom.samples);
                let error = (self.error_fn)(&t_chunk.samples, &buffer)
                    .context("failed to calculate error")?;
                if error < best_error {
                    best_error = error;
                    best_atom_chunk_index = Some(atom);
                }
                buffer.subtract(&atom.samples);
            }

            if let Some(chunk) = best_atom_chunk_index {
                self.candidates[t_id] = Some(AtomSearchResult::Found {
                    details: ImprovementDetails {
                        chunk: t_id,
                        atom_index: chunk.atom_id,
                        atom_chunk: chunk.clone(),
                        chunk_old_error: old_error,
                        chunk_new_error: best_error,
                    },
                })
            } else {
                self.candidates[t_id] = Some(AtomSearchResult::NoImprovement)
            }

            new_candidates_found += 1;
        }

        debug!(
            "populated {} new candidates, {} possible improvements",
            new_candidates_found,
            self.candidates
                .iter()
                .filter(|x| x.is_some()
                    && matches!(x.as_ref().unwrap(), AtomSearchResult::Found { details: _ }))
                .count()
        );
        Ok(())
    }
}

// fn add_atom(output: &mut Samples, atom: &Samples, start: usize) {
//     output
//         .to_vec()
//         .iter_mut()
//         .skip(start)
//         .zip(atom.to_vec().iter())
//         .for_each(|(o, a)| *o += a);
// }

// fn subtract_atom(output: &mut Samples, atom: &Samples, start: usize) {
//     output
//         .to_vec()
//         .iter_mut()
//         .skip(start)
//         .zip(atom.to_vec().iter())
//         .for_each(|(o, a)| *o -= a);
// }
