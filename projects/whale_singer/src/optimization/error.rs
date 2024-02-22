use std::{collections::HashMap, sync::Arc};

use anyhow::bail;
use itertools::Itertools;
use rustfft::{
    num_complex::{Complex, ComplexFloat},
    num_traits::Zero,
    Fft, FftPlanner,
};

use crate::samples::Samples;

pub(crate) struct ErrorCalculator {
    planner: FftPlanner<f32>,
    forward: HashMap<usize, Arc<dyn Fft<f32>>>,
    inverse: HashMap<usize, Arc<dyn Fft<f32>>>,
}

impl Default for ErrorCalculator {
    fn default() -> Self {
        Self {
            planner: FftPlanner::<f32>::new(),
            forward: Default::default(),
            inverse: Default::default(),
        }
    }
}

impl ErrorCalculator {
    fn get_forward(&mut self, len: usize) -> Arc<dyn Fft<f32>> {
        match self.forward.entry(len) {
            std::collections::hash_map::Entry::Occupied(x) => x.get().clone(),
            std::collections::hash_map::Entry::Vacant(x) => {
                x.insert(self.planner.plan_fft_forward(len)).clone()
            }
        }
    }

    fn get_inverse(&mut self, len: usize) -> Arc<dyn Fft<f32>> {
        match self.inverse.entry(len) {
            std::collections::hash_map::Entry::Occupied(x) => x.get().clone(),
            std::collections::hash_map::Entry::Vacant(x) => {
                x.insert(self.planner.plan_fft_inverse(len)).clone()
            }
        }
    }

    /// https://stackoverflow.com/questions/20644599/similarity-between-two-signals-looking-for-simple-measure
    pub(super) fn weighted_error(
        &mut self,
        reference: &Samples,
        input: &Samples,
    ) -> anyhow::Result<f64> {
        let a = &reference.clone().to_vec();
        let b = &input.clone().to_vec();

        // time error
        let ref_time = reference.auto_correlate();
        let inp_time = reference.cross_correlation(input);
        let diff_time = (ref_time - inp_time).abs() as f64;

        // freq error
        let ref_freq = self.cross_correlation_complex(reference.fft(), reference.fft());
        let inp_freq = self.cross_correlation_complex(reference.fft(), input.fft());
        let diff_freq = (ref_freq - inp_freq).abs() as f64;

        // power error
        let ref_power: f32 = a.iter().map(|x| x.powi(2)).sum();
        let inp_power: f32 = b.iter().map(|x| x.powi(2)).sum();
        let diff_power = (ref_power - inp_power).abs() as f64;

        const TIME_WEIGHT: f64 = 1.0;
        const FREQ_WEIGHT: f64 = 1.0;
        const POWER_WEIGHT: f64 = 1.0;

        Ok(diff_time * TIME_WEIGHT + diff_freq * FREQ_WEIGHT + diff_power * POWER_WEIGHT)
    }

    fn cross_correlation_full(&mut self, a: &[f32], b: &[f32]) -> Vec<f32> {
        self.cross_correlation_full_complex(
            &a.iter().map(|&x| Complex::new(x, 0.0)).collect_vec(),
            &b.iter().map(|&x| Complex::new(x, 0.0)).collect_vec(),
        )
        .into_iter()
        .map(|x| x.re())
        .collect_vec()
    }

    /// Compute the cross correlation of a and b
    ///
    /// adapted from https://dsp.stackexchange.com/questions/736/how-do-i-implement-cross-correlation-to-prove-two-audio-files-are-similar
    ///
    /// todo: could be sped up by using a realfft only library
    fn cross_correlation_full_complex(
        &mut self,
        a: &[Complex<f32>],
        b: &[Complex<f32>],
    ) -> Vec<Complex<f32>> {
        // pad the vectors so they seem to go to 0 in the long term
        let len = a.len() + b.len() - 1;

        let fft = self.get_forward(len);

        let mut a_buf = vec![Complex::zero(); len];
        a_buf[..a.len()].copy_from_slice(a);
        fft.process(&mut a_buf);

        let mut b_buf = vec![Complex::zero(); len];
        b_buf[len - b.len()..].copy_from_slice(b);
        b_buf.reverse(); // reverse the input rather than using the complex conjugate
        fft.process(&mut b_buf);

        a_buf
            .iter_mut()
            .zip(b_buf.iter())
            .for_each(|(a, b)| *a *= b);

        let ffti = self.get_inverse(len);
        ffti.process(&mut a_buf);
        a_buf.iter_mut().for_each(|x| *x /= len as f32);

        a_buf
    }

    // cross correlation similar to numpy valid mode when both arrays are equal length
    pub fn cross_correlation(&mut self, a: &[f32], b: &[f32]) -> f32 {
        assert_eq!(a.len(), b.len()); // need to figure out how this works for unqueal samples
        self.cross_correlation_full(a, b)[a.len() - 1]
    }

    // cross correlation similar to numpy valid mode when both arrays are equal length
    fn cross_correlation_complex(
        &mut self,
        a: &[Complex<f32>],
        b: &[Complex<f32>],
    ) -> Complex<f32> {
        assert_eq!(a.len(), b.len()); // need to figure out how this works for unqueal samples
        self.cross_correlation_full_complex(a, b)[a.len() - 1]
    }
}

/// Returns the root mean squared error between two sample combinations
pub(super) fn rms_error(a: &[f32], b: &[f32]) -> anyhow::Result<f64> {
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
        let res = error.cross_correlation_full(&[1.0, 2.0, 3.0, 4.0], &[2.0, 4.0, 6.0]);
        assert!(res
            .into_iter()
            .zip(vec![6., 16., 28., 40., 22., 8.])
            .all(|(a, b)| (a - b).abs() < 0.01));

        // tests from: https://numpy.org/doc/stable/reference/generated/numpy.correlate.html
        let res = error.cross_correlation_full(&[1.0, 2.0, 3.0], &[0.0, 1.0, 0.5]);
        assert!(res
            .into_iter()
            .zip(vec![0.5, 2., 3.5, 3., 0.])
            .all(|(a, b)| (a - b).abs() < 0.01));
        assert_eq!(
            error.cross_correlation(&[1.0, 2.0, 3.0], &[0.0, 1.0, 0.5]),
            3.5
        );

        assert_eq!(
            error.cross_correlation(&[1.0, 2.0, 3.0, 4.0], &[1.0, 2.0, 3.0, 4.0]),
            30.0
        );
    }
}
