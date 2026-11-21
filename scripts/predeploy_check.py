#!/usr/bin/env python3
"""Validate deployment inputs without printing secrets or mutating the network."""
import argparse
import hashlib
import json
import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help="validate a real write deployment")
    parser.add_argument("--confirmed", action="store_true", help="explicitly confirm mutations")
    args = parser.parse_args()
    env = os.environ
    network = env.get("STELLAR_NETWORK", "")
    required = ["STELLAR_NETWORK", "STELLAR_SECRET_KEY", "ADMIN_PUBLIC_KEY", "PLATFORM_FEE_BPS"]
    missing = [name for name in required if not env.get(name)]
    errors = [f"missing {name}" for name in missing]
    if network not in {"testnet", "futurenet", "mainnet", "local"}:
        errors.append("STELLAR_NETWORK must be testnet, futurenet, mainnet, or local")
    if network == "mainnet" and any("mock" in env.get(name, "").lower() for name in ("INVOICE_TOKEN_NAME", "INVOICE_TOKEN_SYMBOL", "MOCK_TOKEN_ID")):
        errors.append("test/mock settings cannot target mainnet")
    if args.write and not args.confirmed:
        errors.append("real deployment requires --confirm")

    artifacts = {}
    defaults = {
        "WASM_INVOICE_ESCROW": "invoice_escrow.wasm",
        "WASM_INVOICE_TOKEN": "invoice_token.wasm",
        "WASM_PAYMENT_DISTRIBUTOR": "payment_distributor.wasm",
    }
    for name, default in defaults.items():
        path = pathlib.Path(env.get(name, f"target/wasm32v1-none/release/{default}"))
        if not path.is_absolute():
            path = ROOT / path
        if not path.exists():
            errors.append(f"missing WASM artifact: {path.relative_to(ROOT) if path.is_relative_to(ROOT) else path}")
            continue
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        expected = env.get(f"{name}_SHA256")
        if expected and expected.lower() != digest:
            errors.append(f"WASM hash mismatch for {name}")
        artifacts[name] = {"path": str(path.relative_to(ROOT)), "sha256": digest}

    report = {"network": network, "write": args.write, "artifacts": artifacts, "errors": errors}
    print(json.dumps(report, sort_keys=True))
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
