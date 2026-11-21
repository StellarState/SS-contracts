# Storage namespaces

Each Soroban contract has an isolated storage namespace. Within a contract,
all persistent and instance keys must use one `StorageKey` or `DataKey` enum.
Variant names are unique and are checked in CI by:

```text
python scripts/check_storage_keys.py
```

Do not reuse a retired variant name for a different value. Keep legacy keys in
the same enum with an explicit `Legacy` prefix until their migration window is
closed; removing a variant is a storage migration, not a cleanup-only change.
