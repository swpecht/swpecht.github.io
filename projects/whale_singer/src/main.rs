use whale_singer::{
    decode::extract_samples,
    encode::save_wav,
    optimization::{add_best_atom, error::rms_error, find_best_match},
};

fn main() {
    // Open the media source.
    let src =
        std::fs::File::open("../../../piano-mp3/piano-mp3/B0.mp3").expect("failed to open media");

    let b0_samples = extract_samples(src).unwrap();

    let src =
        std::fs::File::open("../../../piano-mp3/piano-mp3/A0.mp3").expect("failed to open media");

    let a0_samples = extract_samples(src).unwrap();

    let src =
        std::fs::File::open("../../../piano-mp3/piano-mp3/A1.mp3").expect("failed to open media");

    let a1_samples = extract_samples(src).unwrap();

    let src = std::fs::File::open("simple_target.wav").expect("failed to open media");
    let target = extract_samples(src).unwrap();
    let atoms = vec![a0_samples, a1_samples, b0_samples];
    let output = find_best_match(&target, atoms).unwrap();

    save_wav("output.wav", &output).unwrap();
}
