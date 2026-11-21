#!/usr/bin/env python3
"""Check the generated public entrypoint/event catalog against Rust sources."""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
catalog = ROOT / "docs" / "contract_catalog.md"
entries = []
for source in sorted((ROOT / "contracts").rglob("*.rs")):
    text = source.read_text(encoding="utf-8")
    contract = source.parts[source.parts.index("contracts") + 1]
    for block in re.findall(r"#\[contractimpl[^\]]*\](.*?)(?=\n#\[|\Z)", text, re.S):
        entries.extend(f"| {contract} | method | {name} |" for name in re.findall(r"pub\s+fn\s+([a-zA-Z0-9_]+)\s*\(", block))
    if source.name == "events.rs":
        entries.extend(f"| {contract} | event | {name} |" for name in re.findall(r"pub\s+fn\s+([a-zA-Z0-9_]+)\s*\(", text))

expected = "# Contract ABI and event catalog\n\nGenerated from public `#[contractimpl]` methods and event helpers.\n\n| Contract | Kind | Name |\n|---|---|---|\n" + "\n".join(sorted(set(entries))) + "\n"
if not catalog.exists() or catalog.read_text(encoding="utf-8") != expected:
    print(f"{catalog.relative_to(ROOT)} is stale; run: python scripts/check_contract_catalog.py --write")
    if "--write" in sys.argv:
        catalog.write_text(expected, encoding="utf-8")
        print(f"updated {catalog}")
        return_code = 0
    else:
        return_code = 1
else:
    print("contract catalog is current")
    return_code = 0
sys.exit(return_code)
