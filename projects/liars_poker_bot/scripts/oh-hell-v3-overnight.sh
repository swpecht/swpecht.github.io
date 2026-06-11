#!/usr/bin/env bash
# Overnight v3 pipeline: double the t6-10 data, retrain on all caches,
# eval vs PIMCTS-50. Run via:
#   ./scripts/oh-hell-v3-overnight.sh 2>&1 | kestrel-tail oh-hell-v3 | tee <log>
set -euo pipefail

export LIBTORCH=/home/steven/libtorch
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH=/home/steven/libtorch/lib
export LD_PRELOAD=/home/steven/libtorch/lib/libtorch_cuda.so
export OMP_NUM_THREADS=2
export PYTORCH_CUDA_ALLOC_CONF="expandable_segments:True,max_split_size_mb:512,garbage_collection_threshold:0.8"
export OH_BOOT_THREADS=24 OH_BOOT_ROLLOUTS=50

D=/home/steven/card_platypus/gomcts/oh_hell
mkdir -p $D

echo "=== overnight supplement t6-7 +24000 ==="
[ -f $D/dataset_pimcts50_t6-7_24000.rmp ] || \
  OH_COLLECT_ONLY=1 OH_MIN_TRICKS=6 OH_MAX_TRICKS=7 OH_BOOT_GAMES=24000 \
  OH_BOOT_DATA=$D/dataset_pimcts50_t6-7_24000.rmp \
  ./target/release/examples/oh_hell_gomcts_bootstrap 2>&1

echo "=== overnight supplement t8-10 +12000 ==="
[ -f $D/dataset_pimcts50_t8-10_12000.rmp ] || \
  OH_COLLECT_ONLY=1 OH_MIN_TRICKS=8 OH_MAX_TRICKS=10 OH_BOOT_GAMES=12000 \
  OH_BOOT_DATA=$D/dataset_pimcts50_t8-10_12000.rmp \
  ./target/release/examples/oh_hell_gomcts_bootstrap 2>&1

echo "=== v3 training on all caches ==="
OH_BOOT_DATA=$D/dataset_pimcts50_t1-3_30000.rmp,$D/dataset_pimcts50_t4-5_20000.rmp,$D/dataset_pimcts50_t6-7_4000.rmp,$D/dataset_pimcts50_t8-10_1500.rmp,$D/dataset_pimcts50_t6-7_12000.rmp,$D/dataset_pimcts50_t8-10_6000.rmp,$D/dataset_pimcts50_t6-7_24000.rmp,$D/dataset_pimcts50_t8-10_12000.rmp \
  OH_BOOT_EPOCHS=6 OH_BOOT_BATCH=256 OH_BOOT_LR=1e-4 \
  OH_BOOT_OUT=$D/bootstrap_v3.safetensors \
  ./target/release/examples/oh_hell_gomcts_bootstrap 2>&1

echo "=== v3 eval vs pimcts ==="
OH_WEIGHTS=$D/bootstrap_v3.safetensors OH_MIN_TRICKS=1 OH_MAX_TRICKS=10 \
  OH_GAMES=300 OH_OPPONENT=pimcts \
  ./target/release/examples/oh_hell_gomcts_eval 2>&1

echo "V3 PIPELINE DONE"
