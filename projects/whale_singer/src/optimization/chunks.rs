use itertools::Itertools;

use crate::samples::Samples;

/// Represents a chunk of an atom, used for optimizing
#[derive(Debug, Clone)]
pub struct Chunk {
    // id of the parent atom
    pub atom_id: usize,
    // id of the chunk within the atom
    pub chunk_id: usize,
    pub samples: Samples,
}

/// A sample constructed of chunks

pub(super) struct ConstructedSample {
    chunk_len: usize,
    atom_chunks: Vec<Vec<Chunk>>,
}

impl ConstructedSample {
    pub fn new(chunk_len: usize, num_chunks: usize) -> Self {
        ConstructedSample {
            chunk_len,
            atom_chunks: vec![Vec::new(); num_chunks],
        }
    }

    pub fn atoms(&self, chunk_id: usize) -> &Vec<Chunk> {
        &self.atom_chunks[chunk_id]
    }

    /// Returns the chunk sample made up of all atoms assigned to that chunk
    pub fn chunk_samples(&self, chunk_id: usize) -> Samples {
        let mut samples = Samples::new(vec![0.0; self.chunk_len]);

        for atom_samples in self.atom_chunks[chunk_id].iter().map(|x| &x.samples) {
            samples.add(atom_samples);
        }

        samples
    }

    pub fn add_atom(&mut self, chunk_id: usize, chunk: Chunk) {
        assert!(chunk.samples.len() <= self.chunk_len);
        self.atom_chunks[chunk_id].push(chunk)
    }

    pub fn samples(&self) -> Samples {
        let mut sample_data: Vec<f32> = Vec::new();

        for i in 0..self.atom_chunks.len() {
            let mut new_chunk = Samples::new(vec![0.0; self.chunk_len]);
            new_chunk.add(&self.chunk_samples(i));
            sample_data.extend(new_chunk.to_vec().iter())
        }

        Samples::new(sample_data)
    }
}

/// Converts into chunks, padding chunks with 0s to the chunk length if necessary
pub fn to_chunks(atoms: &[Vec<f32>], chunk_len: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();

    for (atom_id, atom) in atoms.iter().cloned().enumerate() {
        for (chunk_id, chunk_samples) in atom.into_iter().chunks(chunk_len).into_iter().enumerate()
        {
            let mut samples = chunk_samples.collect_vec();
            samples.extend((samples.len()..chunk_len).map(|_| 0.0));
            chunks.push(Chunk {
                atom_id,
                chunk_id,
                samples: samples.into(),
            })
        }
    }

    chunks
}
