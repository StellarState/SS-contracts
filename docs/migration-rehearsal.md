# Migration rehearsal

Run an upgrade plan against a copied JSON state snapshot before deploying it:

```sh
python scripts/rehearse_migration.py snapshot.json migration-plan.json
```

Snapshots contain `schemaVersion` and a `state` object. Plans contain an ordered
`steps` array; every step declares `from`, `to`, optional `requiredKeys`, and
optional `set` values. `targetVersion` and `finalRequiredKeys` describe the
expected final invariants.

The command never writes the snapshot. It prints machine-readable JSON and exits
non-zero when a step is missing, schema versions are discontinuous, or required
keys are absent.
