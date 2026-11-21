#!/usr/bin/env python3
"""Reject duplicate storage-key variants and reserve the contract key namespace."""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
errors = []
for source in sorted((ROOT / "contracts").rglob("*.rs")):
    text = source.read_text(encoding="utf-8")
    for match in re.finditer(r"(?:pub\s+)?enum\s+(?:StorageKey|DataKey)\s*\{(.*?)\n\}", text, re.S):
        variants = re.findall(r"^\s*([A-Z][A-Za-z0-9_]*)\s*(?:\([^\n]*\))?\s*,?\s*(?://.*)?$", match.group(1), re.M)
        duplicates = sorted({name for name in variants if variants.count(name) > 1})
        if duplicates:
            errors.append(f"{source.relative_to(ROOT)}: duplicate storage variants: {', '.join(duplicates)}")
        if not variants:
            errors.append(f"{source.relative_to(ROOT)}: storage enum has no variants")

if errors:
    print("storage key validation failed:", *errors, sep="\n  ")
    sys.exit(1)
print("storage key validation passed")
