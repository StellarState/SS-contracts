#!/usr/bin/env bash
set -euo pipefail

# Keep the supported sizes explicit so CI output is comparable across toolchain upgrades.
for size in 1 5 10 20; do
  echo "batch_size=$size"
  cargo test --package payment-distributor --release -- --nocapture --test-threads=1
done
