use whale_singer::{decode::extract_samples, encode::save_wav};

fn main() {
    // Get the first command line argument.
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("file path not provided");

    // Open the media source.
    let src = std::fs::File::open(path).expect("failed to open media");

    let samples = extract_samples(src).unwrap();
    save_wav("sine.wav", samples).unwrap();
}
