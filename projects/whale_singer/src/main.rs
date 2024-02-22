use std::env::{self};

use whale_singer::{
    app::run_app,
    decode::extract_samples,
    encode::{save_wav, SAMPLE_RATE},
};

use color_eyre::Result;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // just the binary name
    if args.len() == 1 {
        return run_app();
    } else if args[1] == "scratch" {
        run_scratch();
        return color_eyre::Result::Ok(());
    }

    panic!("invalid arguments: {:?}", args);
}

fn run_scratch() {
    // let src =
    //     std::fs::File::open("../../../piano-mp3/piano-mp3/B2.mp3").expect("failed to open media");

    // let a1_samples = extract_samples(src).unwrap();
    // let mut buffer = a1_samples
    //     .clone()
    //     .into_iter()
    //     .map(|x| Complex::new(x, 0.0))
    //     .collect_vec();
    // buffer.truncate(SAMPLE_RATE);

    // let mut planner = FftPlanner::<f32>::new();

    // let fft = planner.plan_fft_forward(buffer.len() / 2);
    // fft.process(&mut buffer);

    // println!("{}", buffer.len());
    // for result in &buffer {
    //     println!("{}", result.norm());
    // }

    let src = std::fs::File::open("./im_different.mp3").expect("failed to open media");

    let mut samples = extract_samples(src).unwrap();
    samples = samples[SAMPLE_RATE * 4..SAMPLE_RATE * 15].to_vec();
    save_wav("./im_different_sample.wav", &samples).unwrap();
}

// fn main() {
//     // Open the media source.
//     let src =
//         std::fs::File::open("../../../piano-mp3/piano-mp3/B0.mp3").expect("failed to open media");

//     let b0_samples = extract_samples(src).unwrap();

//     let src =
//         std::fs::File::open("../../../piano-mp3/piano-mp3/A0.mp3").expect("failed to open media");

//     let a0_samples = extract_samples(src).unwrap();

//

//     // let atoms = vec![a0_samples, a1_samples, b0_samples];
//     // let output = find_best_match(&target, atoms).unwrap();

//     // save_wav("output.wav", &output).unwrap();
// }
