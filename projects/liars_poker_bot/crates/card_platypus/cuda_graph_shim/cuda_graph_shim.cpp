// Minimal C ABI wrapper around at::cuda::CUDAGraph for use from Rust.
// Wraps capture/replay of a forward pass so the entire model executes
// as one CUDA launch on WSL2 (where per-launch D3DKMT overhead is the
// dominant cost — see plans/epimc-gomcts-implementation.md).
//
// Lifecycle:
//   1. set the current CUDA stream to a non-default one (graph-capture
//      requirement) via cgs_use_pooled_stream()
//   2. cgs_new() → CUDAGraph
//   3. warmup forward passes on the to-be-static input
//   4. cgs_capture_begin(g)
//   5. run forward (tensors created during capture become persistent)
//   6. cgs_capture_end(g)
//   7. for each inference: copy real input into the static input tensor,
//      cgs_replay(g), read static outputs

#include <ATen/cuda/CUDAGraph.h>
#include <ATen/cuda/CUDAEvent.h>
#include <c10/cuda/CUDAStream.h>
#include <c10/cuda/CUDACachingAllocator.h>

extern "C" {

// Replace the current stream with one from the pool (graph capture
// cannot run on the default stream).
void cgs_use_pooled_stream() {
    c10::cuda::CUDAStream s = c10::cuda::getStreamFromPool(false);
    c10::cuda::setCurrentCUDAStream(s);
}

void* cgs_new() {
    return new at::cuda::CUDAGraph();
}

void cgs_free(void* g) {
    delete static_cast<at::cuda::CUDAGraph*>(g);
}

void cgs_capture_begin(void* g) {
    static_cast<at::cuda::CUDAGraph*>(g)->capture_begin();
}

void cgs_capture_end(void* g) {
    static_cast<at::cuda::CUDAGraph*>(g)->capture_end();
}

void cgs_replay(void* g) {
    static_cast<at::cuda::CUDAGraph*>(g)->replay();
}

// Return every unused cached block to the CUDA driver. PyTorch's
// CUDACachingAllocator keeps freed allocations in its own pool so
// future tch ops avoid the cudaMalloc round-trip; over a long training
// run that pool grows and fragments, eventually thrashing on every
// allocation. Calling this between phases (typically after each iter's
// snapshot hydrate / between self-play and pop self-play) gives the
// allocator a clean slate without restarting the process.
void cgs_empty_cache() {
    c10::cuda::CUDACachingAllocator::emptyCache();
}

} // extern "C"
