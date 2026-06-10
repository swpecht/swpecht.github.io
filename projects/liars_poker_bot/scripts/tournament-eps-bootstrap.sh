#!/usr/bin/env bash
# Difficulty tournament: ε-bootstrap gomcts vs cfr0 / pimcts / random.
# Target metric (the project goal): point share vs cfr0 — was 21.2% with
# the random bootstrap; >50% = "more powerful than cfr0".
set -euo pipefail

export LIBTORCH=/home/steven/libtorch
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH=/home/steven/libtorch/lib
export LD_PRELOAD=/home/steven/libtorch/lib/libtorch_cuda.so
export OMP_NUM_THREADS=2

export EUCHRE_GOMCTS_WEIGHTS=${EUCHRE_GOMCTS_WEIGHTS:-/home/steven/card_platypus/gomcts/bootstrap.safetensors}
export EUCHRE_GOMCTS_CONFIG=paper
export EUCHRE_GOMCTS_INFER=${EUCHRE_GOMCTS_INFER:-gated}
export EUCHRE_GOMCTS_LAMBDA=${EUCHRE_GOMCTS_LAMBDA:-0.05}
export EUCHRE_GOMCTS_TEMP=${EUCHRE_GOMCTS_TEMP:-0.05}
export EUCHRE_GOMCTS_ITER=${EUCHRE_GOMCTS_ITER:-16}
export BENCH_MATCHES=${BENCH_MATCHES:-30}
export BENCH_AGENTS=${BENCH_AGENTS:-gomcts,cfr0,pimcts,random}

./target/release/examples/euchre_difficulty_benchmark
