#!/usr/bin/env bash
# =============================================================================
# Secret Scanning Allowlist Review Automation
# =============================================================================
#
# Validates secret scanning allowlist entries for:
# - Required issue/ticket ID for each entry
# - No raw secret values in allowlist config
# - Expiration dates and review ownership
# - Malformed or expired exceptions
#
# Usage:
#   bash scripts/secret-scan-allowlist.sh [check|summarize] [allowlist_file]
#
# Arguments:
#   check       Validate allowlist entries (exit code 1 on failure)
#   summarize   Produce redacted summary of allowlist entries
#   allowlist_file  Path to allowlist YAML/JSON file (default: .github/secret-scan-allowlist.yml)
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

ALLOWLIST_FILE="${2:-${REPO_ROOT}/.github/secret-scan-allowlist.yml}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()    { echo -e "${CYAN}[INFO]${NC}  $*"; }
success() { echo -e "${GREEN}[OK]${NC}    $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
die()     { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

command -v yq >/dev/null 2>&1 || die "yq not found. Install with: brew install yq or apt-get install yq"
command -v date >/dev/null 2>&1 || die "date not found"

validate_allowlist() {
    local file="$1"
    local errors=0

    [[ -f "$file" ]] || die "Allowlist file not found: $file"

    local count
    count=$(yq '.allowlist | length' "$file" 2>/dev/null || echo 0)

    if [[ "$count" -eq 0 ]]; then
        info "No allowlist entries found"
        return 0
    fi

    info "Validating $count allowlist entries..."

    for i in $(seq 0 "$((count - 1))"); do
        local pattern
        local issue_id
        local expires
        local owner
        local description
        local secret_value

        pattern=$(yq ".allowlist[$i].pattern" "$file")
        issue_id=$(yq ".allowlist[$i].issue_id" "$file")
        expires=$(yq ".allowlist[$i].expires" "$file")
        owner=$(yq ".allowlist[$i].owner" "$file")
        description=$(yq ".allowlist[$i].description" "$file")
        secret_value=$(yq ".allowlist[$i].secret_value" "$file")

        [[ "$pattern" != "null" && -n "$pattern" ]] || { warn "Entry $i: missing pattern"; ((errors++)); }
        [[ "$issue_id" != "null" && -n "$issue_id" ]] || { warn "Entry $i: missing required issue_id (ticket/issue reference)"; ((errors++)); }
        [[ "$expires" != "null" && -n "$expires" ]] || { warn "Entry $i: missing required expiration date"; ((errors++)); }
        [[ "$owner" != "null" && -n "$owner" ]] || { warn "Entry $i: missing required owner"; ((errors++)); }
        [[ "$description" != "null" && -n "$description" ]] || { warn "Entry $i: missing description"; ((errors++)); }

        if [[ "$secret_value" != "null" && -n "$secret_value" ]]; then
            warn "Entry $i: raw secret_value found in allowlist - REJECTED (secrets must not be stored in config)"
            ((errors++))
        fi

        if [[ "$expires" != "null" && -n "$expires" ]]; then
            local exp_epoch
            exp_epoch=$(date -d "$expires" +%s 2>/dev/null || date -j -f "%Y-%m-%d" "$expires" +%s 2>/dev/null || echo 0)
            local now_epoch
            now_epoch=$(date +%s)
            if [[ "$exp_epoch" -lt "$now_epoch" ]]; then
                warn "Entry $i: expiration date $expires has PASSED (expired)"
                ((errors++))
            fi
        fi

        if [[ "$issue_id" != "null" && -n "$issue_id" ]]; then
            if [[ ! "$issue_id" =~ ^(#[0-9]+|[A-Z]+-[0-9]+|https://github.com/.+/issues/[0-9]+)$ ]]; then
                warn "Entry $i: issue_id '$issue_id' does not match expected format (#123, PROJ-456, or GitHub issue URL)"
                ((errors++))
            fi
        fi
    done

    return $errors
}

summarize_allowlist() {
    local file="$1"

    [[ -f "$file" ]] || die "Allowlist file not found: $file"

    local count
    count=$(yq '.allowlist | length' "$file" 2>/dev/null || echo 0)

    echo "═══════════════════════════════════════════════════════"
    echo "  Secret Scanning Allowlist Summary"
    echo "═══════════════════════════════════════════════════════"
    echo ""
    echo "Total entries: $count"
    echo ""

    if [[ "$count" -eq 0 ]]; then
        echo "No entries."
        return 0
    fi

    printf '%-4s %-40s %-15s %-12s %-20s %s\n' "IDX" "PATTERN (truncated)" "ISSUE_ID" "EXPIRES" "OWNER" "DESCRIPTION"
    printf '%-4s %-40s %-15s %-12s %-20s %s\n' "---" "----------------------------------------" "---------------" "------------" "--------------------" "-----------"

    for i in $(seq 0 "$((count - 1))"); do
        local pattern
        local issue_id
        local expires
        local owner
        local description

        pattern=$(yq ".allowlist[$i].pattern" "$file")
        issue_id=$(yq ".allowlist[$i].issue_id" "$file")
        expires=$(yq ".allowlist[$i].expires" "$file")
        owner=$(yq ".allowlist[$i].owner" "$file")
        description=$(yq ".allowlist[$i].description" "$file")

        local pattern_short="${pattern:0:38}"
        [[ ${#pattern} -gt 38 ]] && pattern_short="${pattern_short}.."

        printf '%-4d %-40s %-15s %-12s %-20s %s\n' "$i" "$pattern_short" "$issue_id" "$expires" "$owner" "$description"
    done

    echo ""
    echo "═══════════════════════════════════════════════════════"
}

main() {
    local action="${1:-check}"

    case "$action" in
        check)
            info "Checking secret scanning allowlist: $ALLOWLIST_FILE"
            validate_allowlist "$ALLOWLIST_FILE"
            success "Allowlist validation passed"
            ;;
        summarize)
            summarize_allowlist "$ALLOWLIST_FILE"
            ;;
        *)
            die "Usage: $0 [check|summarize] [allowlist_file]"
            ;;
    esac
}

main "$@"