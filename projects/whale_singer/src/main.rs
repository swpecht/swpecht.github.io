use whale_singer::decode::extract_samples;

fn main() {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create("sine.wav", spec).unwrap();

    // Get the first command line argument.
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("file path not provided");

    // Open the media source.
    let src = std::fs::File::open(path).expect("failed to open media");

    let samples = extract_samples(src).unwrap();
    samples
        .clone()
        .into_iter()
        .for_each(|x| writer.write_sample(x).unwrap());
}
