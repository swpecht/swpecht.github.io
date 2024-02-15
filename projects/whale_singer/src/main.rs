use whale_singer::app::run_app;

use color_eyre::Result;

fn main() -> Result<()> {
    run_app()
}

// fn main() {
//     // Open the media source.
//     let src =
//         std::fs::File::open("../../../piano-mp3/piano-mp3/B0.mp3").expect("failed to open media");

//     let b0_samples = extract_samples(src).unwrap();

//     let src =
//         std::fs::File::open("../../../piano-mp3/piano-mp3/A0.mp3").expect("failed to open media");

//     let a0_samples = extract_samples(src).unwrap();

//     let src =
//         std::fs::File::open("../../../piano-mp3/piano-mp3/A1.mp3").expect("failed to open media");

//     let a1_samples = extract_samples(src).unwrap();

//     // let mut planner = FftPlanner::<f32>::new();

//     // let mut buffer = a1_samples
//     //     .clone()
//     //     .into_iter()
//     //     .map(|x| Complex::new(x, 0.0))
//     //     .collect_vec();
//     // let fft = planner.plan_fft_forward(a1_samples.len());
//     // fft.process(&mut buffer);
//     // println!("{:?}", buffer);

//

//     // let atoms = vec![a0_samples, a1_samples, b0_samples];
//     // let output = find_best_match(&target, atoms).unwrap();

//     // save_wav("output.wav", &output).unwrap();
// }
