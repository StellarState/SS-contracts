# Contract operations checks

## Dependency health (#765)

Run `scripts/dependency-health.sh` before the first transfer after deployment. It
performs bounded, non-mutating reads against all three configured contracts and
fails with the dependency name when an ID, network, initialization state, or RPC
probe is invalid. `DEPENDENCY_PROBE_RETRIES` is capped at 10.

## Allowlist snapshots (#768)

`scripts/allowlist-digest.sh` canonicalizes an address set by sorting and
deduplicating it before hashing. Store the resulting digest together with a
monotonic revision when wiring an allowlist-sensitive contract; callers should
reject a stale revision or digest rather than silently authorizing against an
older set.

## Upgrade replay coverage (#772)

Upgrade integration tests must exercise approval, expiry, cancellation, repeated
execution, frozen forwarding, authorization, state continuity, and incompatible
logic. Keep those cases in the contract's integration test module so they run
with `cargo test` and do not depend on a live network.

## Batch settlement limits (#774)

`scripts/benchmark-batch-settlement.sh` records the supported benchmark points
(1, 5, 10, and 20) using the pinned Rust toolchain. A production batch endpoint
must reject a larger request before any state mutation; publish the resulting
CPU and footprint measurements as a CI artifact before raising the limit.
