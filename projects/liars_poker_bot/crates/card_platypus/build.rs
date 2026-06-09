// build.rs: compile the cuda_graph_shim C++ wrapper when the
// `tch_spike` feature is on. The shim binds at::cuda::CUDAGraph from
// libtorch so we can capture / replay the transformer forward pass —
// the main lever for the WSL2 per-launch overhead bottleneck.

fn main() {
    #[cfg(feature = "tch_spike")]
    build_cuda_graph_shim();
}

#[cfg(feature = "tch_spike")]
fn build_cuda_graph_shim() {
    use std::env;
    use std::path::PathBuf;

    let libtorch = env::var("LIBTORCH")
        .expect("LIBTORCH env var required to build cuda_graph_shim");
    let libtorch = PathBuf::from(libtorch);
    let include = libtorch.join("include");
    let include_api = libtorch.join("include/torch/csrc/api/include");
    let lib_dir = libtorch.join("lib");

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .file("cuda_graph_shim/cuda_graph_shim.cpp")
        .include(&include)
        .include(&include_api)
        // libtorch is built with _GLIBCXX_USE_CXX11_ABI=1 on Linux.
        .define("_GLIBCXX_USE_CXX11_ABI", "1")
        // Suppress warnings inside libtorch headers that we can't fix.
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-deprecated-declarations")
        .compile("cuda_graph_shim");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=torch");
    println!("cargo:rustc-link-lib=torch_cpu");
    println!("cargo:rustc-link-lib=torch_cuda");
    println!("cargo:rustc-link-lib=c10");
    println!("cargo:rustc-link-lib=c10_cuda");
    println!("cargo:rerun-if-changed=cuda_graph_shim/cuda_graph_shim.cpp");
    println!("cargo:rerun-if-env-changed=LIBTORCH");
}
