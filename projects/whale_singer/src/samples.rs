use itertools::Itertools;
use rustfft::{
    num_complex::{Complex, ComplexFloat},
    num_traits::Zero,
    FftPlanner,
};

#[derive(Clone, Debug, Default)]
pub struct Samples {
    data: Vec<f32>,
    /// the fft of the data
    fft: Vec<Complex<f32>>,
    /// the fft of the fft of the data
    fft_fft: Vec<Complex<f32>>,
    autocor: Option<f32>,
}

impl Samples {
    pub fn new(data: Vec<f32>) -> Self {
        let (fft, fft_fft) = calculate_ffts(&data);

        Samples {
            fft,
            fft_fft,
            data,
            autocor: None,
        }
    }

    pub fn add(&mut self, other: &Self) {
        if self.len() == other.len() {
            // fast path where we can add the ffts as well
            self.data
                .iter_mut()
                .zip(other.data.iter())
                .for_each(|(a, b)| *a += b);
            self.fft
                .iter_mut()
                .zip(other.fft.iter())
                .for_each(|(a, b)| *a += b);
            self.fft_fft
                .iter_mut()
                .zip(other.fft_fft.iter())
                .for_each(|(a, b)| *a += b);
            self.autocor = None;
        } else {
            let items_to_add =
                self.data.len().max(other.data.len()) - self.data.len().min(other.data.len());
            self.data.extend((0..items_to_add).map(|_| 0.0));
            self.data
                .iter_mut()
                .zip(other.data.iter())
                .for_each(|(a, b)| *a += b);

            self.update_cache();
        }
    }

    pub fn subtract(&mut self, other: &Self) {
        if self.len() == other.len() {
            // fast path where we can subtract the ffts as well as the signal
            self.data
                .iter_mut()
                .zip(other.data.iter())
                .for_each(|(a, b)| *a -= b);
            self.fft
                .iter_mut()
                .zip(other.fft.iter())
                .for_each(|(a, b)| *a -= b);
            self.fft_fft
                .iter_mut()
                .zip(other.fft_fft.iter())
                .for_each(|(a, b)| *a -= b);
            self.autocor = None;
        } else {
            let items_to_add =
                self.data.len().max(other.data.len()) - self.data.len().min(other.data.len());
            self.data.extend((0..items_to_add).map(|_| 0.0));
            self.data
                .iter_mut()
                .zip(other.data.iter())
                .for_each(|(a, b)| *a -= b);

            self.update_cache();
        }
    }

    pub fn truncate(&mut self, len: usize) {
        if self.data.len() == len {
            return;
        }

        self.data.truncate(len);

        self.update_cache();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_vec(self) -> Vec<f32> {
        self.data
    }

    pub fn fft(&self) -> &Vec<Complex<f32>> {
        &self.fft
    }

    /// Fast fourier transform of the fast forier transform. Useful for calculating the cross corelation of frequency
    pub fn fft_fft(&self) -> &Vec<Complex<f32>> {
        &self.fft_fft
    }

    pub fn cross_correlation(&self, other: &Self) -> f32 {
        calculate_cross_correlation(self.fft().clone(), other.fft())
    }

    /// Resets all the caches since the underlying data has changed
    fn update_cache(&mut self) {
        (self.fft, self.fft_fft) = calculate_ffts(&self.data);
        self.autocor = None;
    }
}

impl From<Samples> for Vec<f32> {
    fn from(value: Samples) -> Self {
        value.data
    }
}

impl From<Vec<f32>> for Samples {
    fn from(value: Vec<f32>) -> Self {
        Samples::new(value)
    }
}

/// Returns the (fft, fft_fft)
fn calculate_ffts(data: &[f32]) -> (Vec<Complex<f32>>, Vec<Complex<f32>>) {
    let mut planner = FftPlanner::<f32>::new();

    let len = 2 * data.len() - 1;
    let mut fft_buf = data.iter().map(|&x| Complex::new(x, 0.0)).collect_vec();
    fft_buf.extend((fft_buf.len()..len).map(|_| Complex::zero()));

    let fft = planner.plan_fft_forward(len);

    fft.process(&mut fft_buf);

    let mut fft2_buf = fft_buf.clone();
    fft.process(&mut fft2_buf);

    (fft_buf, fft2_buf)
}

fn calculate_cross_correlation(
    mut fft_ref: Vec<Complex<f32>>,
    fft_input: &Vec<Complex<f32>>,
) -> f32 {
    assert_eq!(fft_ref.len(), fft_input.len());

    // multiply the fft by it's conjugate for cross correlation
    fft_ref
        .iter_mut()
        .zip(fft_input)
        .for_each(|(a, b)| *a *= b.conj());

    let mut planner = FftPlanner::<f32>::new();
    let len = fft_ref.len();
    let ffti = planner.plan_fft_inverse(len);
    ffti.process(&mut fft_ref);
    fft_ref.iter_mut().for_each(|x| *x /= len as f32);
    // fft[(fft.len() + 1) / 2 - 1].re()
    fft_ref[0].re()
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_cross_correlation() {
        let mut a = Samples::new(vec![1.0, 2.0, 3.0, 4.0]);
        let (should_fft, _) = calculate_ffts(&a.data.to_vec());
        assert_eq!(a.fft(), &should_fft);
        a.add(&a.clone());
        // since we added the signal to itself, the signal is 2x the old one. The fft should also be 2x the old one
        assert_eq!(a.fft(), &should_fft.iter().map(|x| 2.0 * x).collect_vec());
        // we check explicitly that the fft is the same as the fft of the doubled signal
        assert_eq!(a.fft(), Samples::new(vec![2.0, 4.0, 6.0, 8.0]).fft());

        // tests from: https://numpy.org/doc/stable/reference/generated/numpy.correlate.html
        assert_eq!(
            Samples::new(vec![1.0, 2.0, 3.0]).cross_correlation(&Samples::new(vec![0.0, 1.0, 0.5])),
            3.5
        );

        todo!("add tests, including for add and subtract");
        todo!("figure out why the autocorrelation results are in a different spot than when done normally")
    }
}
