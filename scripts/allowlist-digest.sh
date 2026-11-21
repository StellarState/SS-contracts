#!/usr/bin/env bash
set -euo pipefail

# Produces the same digest for the same ordered set regardless of input order.
# The contract integration can store this value beside its revision number.
if [[ $# -lt 1 ]]; then
  echo "usage: $0 ADDRESS..." >&2
  exit 2
fi

printf '%s\n' "$@" | LC_ALL=C sort -u | sha256sum | awk '{print $1}'
