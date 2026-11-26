#!/usr/bin/env bash
set -euo pipefail

# Non-mutating deployment probes. Each probe is a read-only Soroban invocation.
NETWORK="${STELLAR_NETWORK:-testnet}"
RPC_URL="${STELLAR_RPC_URL:-https://soroban-testnet.stellar.org}"
PASSPHRASE="${STELLAR_NETWORK_PASSPHRASE:-Test SDF Network ; September 2015}"
SOURCE="${STELLAR_SOURCE:-${ADMIN_SECRET:-}}"
RETRIES="${DEPENDENCY_PROBE_RETRIES:-3}"
DELAY="${DEPENDENCY_PROBE_DELAY_SECONDS:-2}"

[[ "$RETRIES" =~ ^[1-9][0-9]*$ ]] || { echo "DEPENDENCY_PROBE_RETRIES must be a positive integer" >&2; exit 2; }
[[ "$RETRIES" -le 10 ]] || { echo "DEPENDENCY_PROBE_RETRIES must be <= 10" >&2; exit 2; }
[[ -n "$SOURCE" ]] || { echo "STELLAR_SOURCE or ADMIN_SECRET is required" >&2; exit 2; }

probe() {
  local name="$1" id="$2" function="$3" attempt output
  [[ "$id" =~ ^C[A-Z2-7]{55}$ ]] || { echo "[FAIL] $name: invalid contract id" >&2; return 1; }
  for ((attempt=1; attempt<=RETRIES; attempt++)); do
    if output=$(stellar contract invoke --id "$id" --source "$SOURCE" --network "$NETWORK" \
      --rpc-url "$RPC_URL" --network-passphrase "$PASSPHRASE" --send no -- "$function" 2>&1); then
      echo "[PASS] $name: $output"
      return 0
    fi
    (( attempt == RETRIES )) || sleep "$DELAY"
  done
  echo "[FAIL] $name: probe failed after $RETRIES attempts" >&2
  return 1
}

failures=0
probe invoice-escrow "${ESCROW_CONTRACT_ID:-}" get_admin || failures=$((failures + 1))
probe invoice-token "${INV_TOKEN_CONTRACT_ID:-}" decimals || failures=$((failures + 1))
probe payment-distributor "${DISTRIBUTOR_CONTRACT_ID:-}" get_admin || failures=$((failures + 1))

if (( failures > 0 )); then
  echo "dependency health failed: $failures dependency probe(s) did not pass" >&2
  exit 1
fi
echo "dependency health passed"
