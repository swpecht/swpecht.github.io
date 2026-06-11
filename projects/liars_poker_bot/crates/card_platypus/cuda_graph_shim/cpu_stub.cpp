// CPU-only stand-in for cuda_graph_shim.cpp, compiled when the libtorch
// at $LIBTORCH has no libtorch_cuda.so (see build.rs). Satisfies the
// cgs_* symbols the Rust extern block declares without pulling in any
// CUDA dependency, so server binaries link and run against CPU-only
// libtorch.
//
// The harmless toggles (pooled stream, cache, TF32) are no-ops. The
// graph capture/replay entry points abort loudly: they are only
// reachable through explicit `use_graph = true` call sites, and a
// silent no-op there would return stale tensors instead of failing.

#include <cstdio>
#include <cstdlib>

extern "C" {

void cgs_use_pooled_stream() {}

void* cgs_new() {
    return nullptr;
}

void cgs_free(void*) {}

static void cgs_abort_no_cuda(const char* fn) {
    std::fprintf(stderr,
                 "%s: CUDA graph capture is unavailable in a CPU-only "
                 "libtorch build; run with use_graph=false\n",
                 fn);
    std::abort();
}

void cgs_capture_begin(void*) {
    cgs_abort_no_cuda("cgs_capture_begin");
}

void cgs_capture_end(void*) {
    cgs_abort_no_cuda("cgs_capture_end");
}

void cgs_replay(void*) {
    cgs_abort_no_cuda("cgs_replay");
}

void cgs_empty_cache() {}

void cgs_set_allow_tf32_matmul(bool) {}

void cgs_set_allow_tf32_cudnn(bool) {}

} // extern "C"
