use anyhow::bail;
use dssim::Dssim;

use super::RBGA;

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

pub(super) fn dssim_error(a: &RBGA, b: &RBGA) -> anyhow::Result<f64> {
    use rgb::FromSlice;
    let dssim = Dssim::new();
    let a_image = dssim
        .create_image_rgba(a.data[..].as_rgba(), a.width, a.height)
        .unwrap();

    let b_image = dssim
        .create_image_rgba(b.data[..].as_rgba(), b.width, b.height)
        .unwrap();
    let (diff, _) = dssim.compare(&a_image, &b_image);

    Ok(diff.into())
}
