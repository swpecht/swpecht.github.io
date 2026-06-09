#!/usr/bin/env bash
# Download + verify libtorch CUDA 12.4 (PyTorch 2.5.0) into /tmp/libtorch.
# After this completes successfully, set the env vars it prints (or copy
# them into your shell rc) so cargo can find libtorch.
#
# Usage:
#   ./scripts/setup-libtorch.sh
#
# Idempotent: if /tmp/libtorch already contains a libtorch tree it
# verifies and skips the download.

set -euo pipefail

LIBTORCH_DIR="${LIBTORCH_DIR:-/tmp/libtorch}"
# PyTorch 2.5.0 + cu124 build. The cxx11 ABI variant matches the
# default Linux toolchain; the pre-cxx11 ABI variant is the one Python
# wheels use and is incompatible with the rest of our build.
LIBTORCH_URL="${LIBTORCH_URL:-https://download.pytorch.org/libtorch/cu124/libtorch-cxx11-abi-shared-with-deps-2.5.0%2Bcu124.zip}"
LIBTORCH_ZIP="/tmp/libtorch-2.5.0+cu124.zip"

verify_tree() {
  [[ -d "$LIBTORCH_DIR/lib" ]] \
    && [[ -f "$LIBTORCH_DIR/lib/libtorch.so" ]] \
    && [[ -f "$LIBTORCH_DIR/lib/libtorch_cuda.so" ]] \
    && [[ -f "$LIBTORCH_DIR/lib/libc10.so" ]] \
    && [[ -d "$LIBTORCH_DIR/include" ]]
}

if verify_tree; then
  echo "libtorch already present at $LIBTORCH_DIR — skipping download."
else
  if [[ ! -f "$LIBTORCH_ZIP" ]]; then
    echo "Downloading libtorch 2.5.0+cu124 to $LIBTORCH_ZIP …"
    curl -L --fail -o "$LIBTORCH_ZIP" "$LIBTORCH_URL"
  fi
  echo "Extracting to /tmp …"
  rm -rf "$LIBTORCH_DIR"
  unzip -q "$LIBTORCH_ZIP" -d /tmp
  if [[ ! -d "$LIBTORCH_DIR" ]]; then
    echo "ERROR: extraction did not produce $LIBTORCH_DIR" >&2
    exit 1
  fi
  if ! verify_tree; then
    echo "ERROR: $LIBTORCH_DIR is missing expected files" >&2
    exit 1
  fi
  echo "libtorch unpacked at $LIBTORCH_DIR"
fi

cat <<EOF

libtorch ready. Export these (e.g. add to ~/.bashrc or ~/.config/fish/config.fish):

  export LIBTORCH=$LIBTORCH_DIR
  export LIBTORCH_BYPASS_VERSION_CHECK=1
  export LD_LIBRARY_PATH=$LIBTORCH_DIR/lib:\$LD_LIBRARY_PATH
  export LD_PRELOAD=$LIBTORCH_DIR/lib/libtorch_cuda.so

For one-shot use, prefix cargo commands with the same exports:
  LIBTORCH=$LIBTORCH_DIR LIBTORCH_BYPASS_VERSION_CHECK=1 \\
    LD_LIBRARY_PATH=$LIBTORCH_DIR/lib:\$LD_LIBRARY_PATH \\
    cargo build -p card_platypus --release
EOF
