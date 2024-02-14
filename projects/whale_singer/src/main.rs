use whale_singer::{decode::extract_samples, encode::save_wav, optimization::error::rms_error};

fn main() {
    // Open the media source.
    let src =
        std::fs::File::open("../../../piano-mp3/piano-mp3/B0.mp3").expect("failed to open media");

    let b0_samples = extract_samples(src).unwrap();

    let src =
        std::fs::File::open("../../../piano-mp3/piano-mp3/A0.mp3").expect("failed to open media");

    let a0_samples = extract_samples(src).unwrap();
    save_wav("sine.wav", &a0_samples).unwrap();

    println!("{}", rms_error(&a0_samples, &b0_samples).unwrap());
}
