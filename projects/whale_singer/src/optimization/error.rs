use std::{collections::HashMap, sync::Arc};

use anyhow::bail;
use rustfft::{
    num_complex::{Complex, ComplexFloat},
    num_traits::Zero,
    Fft, FftPlanner,
};

use crate::samples::Samples;

use super::chunks::Chunk;

pub(crate) struct ErrorCalculator {
    planner: FftPlanner<f32>,
    inverse: HashMap<usize, Arc<dyn Fft<f32>>>,
    autocor_time_cache: HashMap<usize, f32>,
    autocor_freq_cache: HashMap<usize, Complex<f32>>,
}

impl Default for ErrorCalculator {
    fn default() -> Self {
        Self {
            planner: FftPlanner::<f32>::new(),
            inverse: Default::default(),
            autocor_time_cache: Default::default(),
            autocor_freq_cache: Default::default(),
        }
    }
}

impl ErrorCalculator {
    fn get_inverse(&mut self, len: usize) -> Arc<dyn Fft<f32>> {
        get_or_insert(len, &mut self.inverse, || {
            self.planner.plan_fft_inverse(len)
        })
    }

    fn get_autocor_time(&mut self, ref_chunk: &Chunk) -> f32 {
        if let Some(&cor) = self.autocor_time_cache.get(&ref_chunk.chunk_id) {
            return cor;
        }

        let cor = self.cross_correlation(ref_chunk.samples.fft(), ref_chunk.samples.fft());

        self.autocor_time_cache.insert(ref_chunk.chunk_id, cor.re());
        cor.re()
    }

    fn get_autocor_freq(&mut self, reference: &Chunk) -> Complex<f32> {
        if let Some(&cor) = self.autocor_freq_cache.get(&reference.chunk_id) {
            return cor;
        }

        let cor = self.cross_correlation(reference.samples.fft_fft(), reference.samples.fft_fft());

        self.autocor_freq_cache.insert(reference.chunk_id, cor);
        cor
    }

    /// https://stackoverflow.com/questions/20644599/similarity-between-two-signals-looking-for-simple-measure
    pub(super) fn weighted_error(
        &mut self,
        reference: &Chunk,
        input: &Samples,
    ) -> anyhow::Result<f64> {
        // time error
        let ref_time = self.get_autocor_time(reference); // todo: benchmark to see if sample approach is faster
        let inp_time = self.cross_correlation(reference.samples.fft(), input.fft()); // todo: benchmark to see if sample approach is faster
        let diff_time = (ref_time - inp_time).abs() as f64;

        // freq error
        let ref_freq = self.get_autocor_freq(reference);
        let inp_freq = self.cross_correlation(reference.samples.fft_fft(), input.fft_fft());
        let diff_freq = (ref_freq - inp_freq).abs() as f64;

        // power error
        let ref_power: f32 = reference.samples.data().iter().map(|x| x.powi(2)).sum();
        let inp_power: f32 = input.data().iter().map(|x| x.powi(2)).sum();
        let diff_power = (ref_power - inp_power).abs() as f64;

        const TIME_WEIGHT: f64 = 1.0;
        const FREQ_WEIGHT: f64 = 1.0;
        const POWER_WEIGHT: f64 = 1.0; // todo add back in or normalize

        Ok(diff_time * TIME_WEIGHT + diff_freq * FREQ_WEIGHT + diff_power * POWER_WEIGHT)
    }

    fn cross_correlation(
        &mut self,
        a_fft: &[Complex<f32>],
        b_fft: &[Complex<f32>],
    ) -> Complex<f32> {
        assert_eq!(a_fft.len(), b_fft.len());

        let len = a_fft.len();
        let mut buf = vec![Complex::zero(); len];
        buf.iter_mut()
            .zip(a_fft.iter().zip(b_fft.iter()))
            // corr(a, b) = ifft(fft(a_and_zeros) * conj(fft(b_and_zeros)))
            .for_each(|(buf, (a, b))| *buf = a * b.conj());

        let ffti = self.get_inverse(len);
        ffti.process(&mut buf);
        buf.iter_mut().for_each(|x| *x /= len as f32);

        // todo: why is the fft offset to zero here rather than a.len() - 1?
        buf[0]
    }
}

fn get_or_insert<K, V, F>(key: K, cache: &mut HashMap<K, V>, or_else: F) -> V
where
    K: std::hash::Hash + std::cmp::Eq + std::cmp::PartialEq,
    V: Clone,
    F: FnOnce() -> V,
{
    match cache.entry(key) {
        std::collections::hash_map::Entry::Occupied(x) => x.get().clone(),
        std::collections::hash_map::Entry::Vacant(x) => x.insert(or_else()).clone(),
    }
}

/// Returns the root mean squared error between two sample combinations
pub(super) fn _rms_error(a: &[f32], b: &[f32]) -> anyhow::Result<f64> {
    if a.len() != b.len() {
        bail!("cannot calculate rms error on vectors of different length");
    }

    let mut error = 0.0;

    for (&x, &y) in a.iter().zip(b.iter()) {
        error += (x as f64 - y as f64).powi(2);
    }

    Ok(error)
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn test_cross_correlation() {
        let mut error = ErrorCalculator::default();

        // tests from: https://numpy.org/doc/stable/reference/generated/numpy.correlate.html
        assert_eq!(
            error
                .cross_correlation(
                    Samples::new(vec![1.0, 2.0, 3.0]).fft(),
                    Samples::new(vec![0.0, 1.0, 0.5]).fft()
                )
                .re(),
            3.5
        );

        assert_eq!(
            error
                .cross_correlation(
                    Samples::new(vec![1.0, 2.0, 3.0, 4.0]).fft(),
                    Samples::new(vec![1.0, 2.0, 3.0, 4.0]).fft()
                )
                .re(),
            30.0
        );
    }
}
