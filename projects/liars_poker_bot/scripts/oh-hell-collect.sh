#!/usr/bin/env bash
# Oh Hell pimcts+ε bootstrap data collection, banded by trick count so
# the cheap bands don't starve behind the expensive ones. Each band
# writes its own dataset cache; training merges them via the
# comma-separated OH_BOOT_DATA.
set -euo pipefail

export LIBTORCH=/home/steven/libtorch
export LIBTORCH_BYPASS_VERSION_CHECK=1
export LD_LIBRARY_PATH=/home/steven/libtorch/lib
export LD_PRELOAD=/home/steven/libtorch/lib/libtorch_cuda.so
export OH_BOOT_THREADS=${OH_BOOT_THREADS:-24}
export OH_BOOT_ROLLOUTS=50
export OH_COLLECT_ONLY=1

D=/home/steven/card_platypus/gomcts/oh_hell
mkdir -p $D

run_band () { # min max games
  local min=$1 max=$2 games=$3
  local cache=$D/dataset_pimcts50_t${min}-${max}_${games}.rmp
  if [ -f "$cache" ]; then echo "band t$min-$max exists, skipping"; return; fi
  echo "=== band tricks $min-$max, $games games ==="
  OH_MIN_TRICKS=$min OH_MAX_TRICKS=$max OH_BOOT_GAMES=$games OH_BOOT_DATA=$cache \
    ./target/release/examples/oh_hell_gomcts_bootstrap 2>&1
}

run_band 1 3 30000
run_band 4 5 20000
run_band 6 7 12000
run_band 8 10 9000
echo "ALL BANDS DONE"
