#!/usr/bin/env bash
# Eval the ε-exploration bootstrap checkpoint across inference modes.
# H1 predictions (from plans/epimc-gomcts-implementation.md entry 29):
#   lm     ≈ +0.35  (cfr3-imitation level, like the pure-cfr3 bootstrap)
#   argmax > +0.10  (pure-cfr3 bootstrap was -0.118 — broken V; ε-coverage should fix it)
#   gated  ≥ both   (paper mode: LM filters, V picks)
# Then the best raw mode gets an MCTS-wrapped eval.
set -euo pipefail

export LIBTORCH=/home/steven/libtorch
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH=/home/steven/libtorch/lib
export LD_PRELOAD=/home/steven/libtorch/lib/libtorch_cuda.so
export OMP_NUM_THREADS=2
export PYTORCH_CUDA_ALLOC_CONF="expandable_segments:True,max_split_size_mb:512,garbage_collection_threshold:0.8"

BIN=./target/release/examples/euchre_gomcts_eval
export EU_WEIGHTS=${EU_WEIGHTS:-/home/steven/card_platypus/gomcts/bootstrap.safetensors}
export EU_CONFIG=paper
export EU_GAMES=${EU_GAMES:-2000}

echo "=== [1/4] raw, EU_INFER=lm ==="
EU_INFER=lm EU_SKIP_MCTS=1 $BIN

echo "=== [2/4] raw, EU_INFER=argmax (temp 0.5, log-comparable) ==="
EU_INFER=argmax EU_SKIP_MCTS=1 $BIN

echo "=== [3/4] raw, EU_INFER=gated lambda=0.05 temp=0.05 (paper ArgmaxVal*) ==="
EU_INFER=gated EU_LAMBDA=0.05 EU_TEMP=0.05 EU_SKIP_MCTS=1 $BIN

echo "=== [4/4] MCTS-wrapped (gated, 100 sims — paper tournament budget) ==="
EU_INFER=gated EU_LAMBDA=0.05 EU_TEMP=0.05 EU_MCTS_ITER=100 EU_GAMES=400 $BIN

echo "ALL EVALS DONE"
