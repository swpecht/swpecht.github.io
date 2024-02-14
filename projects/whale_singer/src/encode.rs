use anyhow::Context;

pub const SAMPLE_RATE: usize = 44100;

pub fn save_wav(filename: &str, samples: &[f32]) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(filename, spec).context("failed to create writer")?;
    for &sample in samples {
        writer
            .write_sample(sample)
            .context("failed to write sample")?;
    }

    Ok(())
}
