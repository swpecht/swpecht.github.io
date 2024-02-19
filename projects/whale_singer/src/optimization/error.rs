use anyhow::bail;
use itertools::Itertools;
use realfft::RealFftPlanner;
use rustfft::{
    num_complex::{Complex, ComplexFloat},
    num_traits::Zero,
    FftPlanner,
};

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

/// https://stackoverflow.com/questions/20644599/similarity-between-two-signals-looking-for-simple-measure
pub(super) fn weighted_error(a: &[f32], b: &[f32]) -> anyhow::Result<f64> {
    // time error
    let ref_time = cross_correlation(a, a);
    let inp_time = cross_correlation(a, b);
    let diff_time = (ref_time - inp_time).abs() as f64;

    // freq error
    // todo: implement frequency similarity
    // ref_freq = np.correlate(np.fft.fft(ref_rec),np.fft.fft(ref_rec))
    // inp_freq = np.correlate(np.fft.fft(ref_rec),np.fft.fft(input_rec))
    // diff_freq = abs(ref_freq-inp_freq)
    // let mut planner = FftPlanner::new();
    // let fft = planner.plan_fft_forward(a.len());
    // fft.pr

    // power error
    let ref_power: f32 = a.iter().map(|x| x.powi(2)).sum();
    let inp_power: f32 = b.iter().map(|x| x.powi(2)).sum();
    let diff_power = (ref_power - inp_power).abs() as f64;

    const TIME_WEIGHT: f64 = 1.0;
    const POWER_WEIGHT: f64 = 1.0;

    Ok(diff_time * TIME_WEIGHT + diff_power * POWER_WEIGHT)
}

/// Compute the cross correlation of a and b
///
/// adapted from https://dsp.stackexchange.com/questions/736/how-do-i-implement-cross-correlation-to-prove-two-audio-files-are-similar
///
/// todo: could be sped up by using a realfft only library
fn cross_correlation_full(a: &[f32], b: &[f32]) -> Vec<f32> {
    let mut planner = RealFftPlanner::<f32>::new();

    // pad the vectors so they seem to go to 0 in the long term
    let len = a.len() + b.len() - 1;

    let fft = planner.plan_fft_forward(len);
    let mut input_buf = fft.make_input_vec();
    let mut a_buf = fft.make_output_vec();

    input_buf[..a.len()].copy_from_slice(a);
    let mut scratch = vec![Complex::zero(); len];
    fft.process_with_scratch(&mut input_buf, &mut a_buf, &mut scratch)
        .unwrap();

    let mut input_buf = fft.make_input_vec();
    input_buf[len - b.len()..].copy_from_slice(b);
    input_buf.reverse(); // reverse the input rather than using the complex conjugate
    let mut b_buf = fft.make_output_vec();
    fft.process_with_scratch(&mut input_buf, &mut b_buf, &mut scratch)
        .unwrap();

    a_buf
        .iter_mut()
        .zip(b_buf.iter())
        .for_each(|(a, b)| *a *= b);

    let ffti = planner.plan_fft_inverse(len);
    let mut output = ffti.make_output_vec();
    ffti.process_with_scratch(&mut a_buf, &mut output, &mut scratch)
        .unwrap();
    output.iter_mut().for_each(|x| *x /= len as f32);

    output
}

// cross correlation similar to numpy valid mode when both arrays are equal length
fn cross_correlation(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len()); // need to figure out how this works for unqueal samples
    cross_correlation_full(a, b)[a.len() - 1]
}

#[cfg(test)]
mod tests {

    use super::*;
    #[test]
    fn test_cross_correlation() {
        let res = cross_correlation_full(&[1.0, 2.0, 3.0, 4.0], &[2.0, 4.0, 6.0]);
        assert!(res
            .into_iter()
            .zip(vec![6., 16., 28., 40., 22., 8.])
            .all(|(a, b)| (a - b).abs() < 0.01));

        // tests from: https://numpy.org/doc/stable/reference/generated/numpy.correlate.html
        let res = cross_correlation_full(&[1.0, 2.0, 3.0], &[0.0, 1.0, 0.5]);
        assert!(res
            .into_iter()
            .zip(vec![0.5, 2., 3.5, 3., 0.])
            .all(|(a, b)| (a - b).abs() < 0.01));
        assert_eq!(cross_correlation(&[1.0, 2.0, 3.0], &[0.0, 1.0, 0.5]), 3.5);
    }
}
