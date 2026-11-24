#!/usr/bin/env python3
"""Validate an upgrade migration against a copied state snapshot without mutating it."""

import argparse
import copy
import json
import sys
from pathlib import Path


def rehearse(snapshot: dict, plan: dict) -> dict:
    original = copy.deepcopy(snapshot)
    state = copy.deepcopy(snapshot)
    failures = []
    applied = 0
    current = state.get("schemaVersion")

    for step in plan.get("steps", []):
        if step.get("from") != current:
            failures.append(f"missing migration from schema version {current}")
            break
        for key in step.get("requiredKeys", []):
            if key not in state.get("state", {}):
                failures.append(f"required key is missing: {key}")
        for key, value in step.get("set", {}).items():
            state.setdefault("state", {})[key] = value
        current = step.get("to")
        state["schemaVersion"] = current
        applied += 1

    expected = plan.get("targetVersion")
    if current != expected:
        failures.append(f"target schema version {expected} was not reached")
    for key in plan.get("finalRequiredKeys", []):
        if key not in state.get("state", {}):
            failures.append(f"final key is missing: {key}")

    return {
        "success": not failures,
        "sourceVersion": snapshot.get("schemaVersion"),
        "targetVersion": expected,
        "migrationCount": applied,
        "failures": failures,
        "sourceUnchanged": snapshot == original,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("snapshot", type=Path)
    parser.add_argument("plan", type=Path)
    args = parser.parse_args()
    snapshot = json.loads(args.snapshot.read_text(encoding="utf-8"))
    result = rehearse(snapshot, json.loads(args.plan.read_text(encoding="utf-8")))
    print(json.dumps(result, sort_keys=True))
    return 0 if result["success"] else 1


if __name__ == "__main__":
    sys.exit(main())
