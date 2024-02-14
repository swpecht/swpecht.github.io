use anyhow::Context;

pub fn save_wav(filename: &str, samples: Vec<f32>) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(filename, spec).context("failed to create writer")?;
    for sample in samples {
        writer
            .write_sample(sample)
            .context("failed to write sample")?;
    }

    Ok(())
}
