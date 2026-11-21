#!/usr/bin/env bash
# =============================================================================
# StellarSettle – WASM Packaging & Checksum Script (Deterministic Build)
# =============================================================================
#
# Compiles all Soroban contracts to WASM with reproducible build metadata
# and generates a deterministic SHA256 checksums file for release transparency
# and contract verification.
#
# Usage:
#   bash scripts/package-wasm.sh [output_dir] [--verify]
#
# Arguments:
#   output_dir   Directory to place WASM files, checksums.txt, and metadata.json
#                (default: dist/)
#   --verify     Verify clean rebuild reproduces the same hashes
#
# Requirements:
#   - Rust with wasm32v1-none target installed
#   - sha256sum (coreutils)
#   - jq (for metadata JSON generation)
#   - git (for commit info)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

OUTPUT_DIR="${1:-${REPO_ROOT}/dist}"
VERIFY_REBUILD="${2:-}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
die()     { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Verify required tools
# ---------------------------------------------------------------------------
command -v sha256sum >/dev/null 2>&1 || die "sha256sum not found. Install coreutils."
command -v jq >/dev/null 2>&1 || die "jq not found. Install jq for JSON metadata generation."
command -v git >/dev/null 2>&1 || die "git not found. Required for commit metadata."
command -v cargo >/dev/null 2>&1 || die "cargo not found. Install Rust toolchain."
command -v rustc >/dev/null 2>&1 || die "rustc not found. Install Rust toolchain."

# ---------------------------------------------------------------------------
# Capture build environment metadata for reproducibility
# ---------------------------------------------------------------------------
capture_build_metadata() {
    local commit_hash
    local commit_date
    local rust_version
    local cargo_version
    local target
    local cargo_lock_hash

    commit_hash=$(git rev-parse HEAD 2>/dev/null || echo "unknown")
    commit_date=$(git log -1 --format=%ci 2>/dev/null || echo "unknown")
    rust_version=$(rustc --version 2>/dev/null || echo "unknown")
    cargo_version=$(cargo --version 2>/dev/null || echo "unknown")
    target="wasm32v1-none"
    cargo_lock_hash=$(sha256sum "${REPO_ROOT}/Cargo.lock" 2>/dev/null | cut -d' ' -f1 || echo "unknown")

    # Get dependency versions from Cargo.lock
    local deps_json="{}"
    if [[ -f "${REPO_ROOT}/Cargo.lock" ]]; then
        deps_json=$(jq -n '
          [inputs
           | select(.name and .version)
           | {(.name): .version}]
          | add' "${REPO_ROOT}/Cargo.lock" 2>/dev/null || echo "{}")
    fi

    cat <<EOF
{
  "commit": "${commit_hash}",
  "commit_date": "${commit_date}",
  "rust_version": "${rust_version}",
  "cargo_version": "${cargo_version}",
  "target": "${target}",
  "cargo_lock_sha256": "${cargo_lock_hash}",
  "dependencies": ${deps_json},
  "build_timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "builder": "$(whoami)@$(hostname)"
}
EOF
}

# ---------------------------------------------------------------------------
# Build WASM contracts with deterministic settings
# ---------------------------------------------------------------------------
build_wasm() {
    info "Building WASM contracts with deterministic settings..."
    cd "${REPO_ROOT}"

    # Ensure we're using the pinned toolchain from rust-toolchain.toml
    local toolchain
    toolchain=$(grep '^channel' "${REPO_ROOT}/rust-toolchain.toml" 2>/dev/null | cut -d'"' -f2 || echo "stable")
    info "Using Rust toolchain: ${toolchain}"

    # Build with deterministic settings
    CARGO_BUILD_PIPELINES=1 \
    CARGO_INCREMENTAL=0 \
    RUSTFLAGS="${RUSTFLAGS:-} -C panic=abort -C codegen-units=1" \
    cargo build --release --target wasm32v1-none \
      -p invoice-escrow -p invoice-token -p payment-distributor

    WASM_DIR="target/wasm32v1-none/release"
    REQUIRED_FILES=(
      "invoice_escrow.wasm"
      "invoice_token.wasm"
      "payment_distributor.wasm"
    )

    for wasm in "${REQUIRED_FILES[@]}"; do
      [[ -f "${WASM_DIR}/${wasm}" ]] || die "WASM not found: ${WASM_DIR}/${wasm}"
    done
}

# ---------------------------------------------------------------------------
# Generate SHA256 checksums and metadata
# ---------------------------------------------------------------------------
generate_artifacts() {
    info "Packaging WASM artifacts to ${OUTPUT_DIR}..."
    mkdir -p "${OUTPUT_DIR}"

    for wasm in "${REQUIRED_FILES[@]}"; do
      cp "${WASM_DIR}/${wasm}" "${OUTPUT_DIR}/${wasm}"
      success "Copied ${wasm}"
    done

    info "Generating checksums..."
    cd "${OUTPUT_DIR}"
    sha256sum "${REQUIRED_FILES[@]}" > checksums.txt
    success "checksums.txt generated"

    info "Generating build metadata..."
    capture_build_metadata > "${OUTPUT_DIR}/metadata.json"
    success "metadata.json generated"

    # Display summary
    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo "  Build Metadata"
    echo "═══════════════════════════════════════════════════════"
    cat "${OUTPUT_DIR}/metadata.json" | jq .
    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo "  Checksums"
    echo "═══════════════════════════════════════════════════════"
    cat checksums.txt
    echo ""
    info "Output directory: ${OUTPUT_DIR}"
}

# ---------------------------------------------------------------------------
# Verify clean rebuild reproduces the same hashes
# ---------------------------------------------------------------------------
verify_rebuild() {
    info "Verifying clean rebuild reproduces identical hashes..."
    local temp_dir
    temp_dir=$(mktemp -d)
    trap 'rm -rf "${temp_dir}"' RETURN

    # Save current artifacts
    local original_checksums
    original_checksums=$(cat "${OUTPUT_DIR}/checksums.txt")

    # Clean and rebuild in temp directory
    cd "${REPO_ROOT}"
    cargo clean -p invoice-escrow -p invoice-token -p payment-distributor --target wasm32v1-none 2>/dev/null || true

    # Rebuild
    build_wasm

    # Copy to temp dir and generate checksums
    mkdir -p "${temp_dir}/verify"
    for wasm in "${REQUIRED_FILES[@]}"; do
      cp "${WASM_DIR}/${wasm}" "${temp_dir}/verify/${wasm}"
    done

    cd "${temp_dir}/verify"
    sha256sum "${REQUIRED_FILES[@]}" > rebuild_checksums.txt

    local rebuild_checksums
    rebuild_checksums=$(cat rebuild_checksums.txt)

    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo "  Rebuild Verification"
    echo "═══════════════════════════════════════════════════════"
    echo "Original:"
    echo "${original_checksums}"
    echo ""
    echo "Rebuild:"
    echo "${rebuild_checksums}"
    echo ""

    if [[ "${original_checksums}" == "${rebuild_checksums}" ]]; then
        success "✓ Rebuild VERIFIED: Hashes match exactly"
        return 0
    else
        warn "✗ Rebuild FAILED: Hashes differ!"
        echo ""
        echo "Differences:"
        diff -u <(echo "${original_checksums}") <(echo "${rebuild_checksums}") || true
        return 1
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    info "Starting deterministic WASM packaging..."

    build_wasm
    generate_artifacts

    if [[ "${VERIFY_REBUILD}" == "--verify" ]]; then
        echo ""
        verify_rebuild
    fi

    success "Deterministic WASM packaging complete"
}

main "$@"