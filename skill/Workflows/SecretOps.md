# SecretOps

Read, write, delete, refresh, or list secrets via the op-bridge daemon.

## Prerequisites

The daemon must be running. Check with:

```bash
~/op-bridge/target/debug/op-bridge ping 2>/dev/null || echo "Daemon not running — start it first"
```

If not running, inform the user and suggest starting the daemon (DaemonStart workflow).

## Operations

### GET — Read a secret

```bash
~/op-bridge/target/debug/op-bridge get <NAME>
```

The value is printed to stdout with no trailing newline. Handle it securely — avoid logging the value.

### SET — Write a secret (memory + 1Password)

```bash
~/op-bridge/target/debug/op-bridge set <NAME> <op://vault/item/field> <VALUE>
```

This writes to 1Password first, then updates the in-memory store. If the 1Password write fails, the in-memory store is not updated.

**Important:** The value is passed as a CLI argument. For long or multi-line values, consider using the socket protocol directly via `socat`.

### DELETE — Remove from memory only

```bash
~/op-bridge/target/debug/op-bridge delete <NAME>
```

Removes the secret from the in-memory store and zeroizes it. Does **NOT** delete anything from 1Password. Use this to revoke runtime access to a secret without affecting the source of truth.

### REFRESH — Re-resolve all secrets from 1Password

```bash
~/op-bridge/target/debug/op-bridge refresh
```

Sends SIGHUP to the daemon, which re-resolves all `OP_*_REF` secrets from 1Password. Useful after rotating credentials upstream. The daemon reads its PID from the PID file (default: `/tmp/op-bridge.pid`).

### LIST — Show all loaded secret names

```bash
~/op-bridge/target/debug/op-bridge list
```

Prints one name per line. Does not expose values.

## Troubleshooting

If operations fail, check:

1. **Socket exists:** `test -S /tmp/op-bridge.sock`
2. **Daemon is alive:** `op-bridge ping`
3. **Daemon logs:** Restart with `--log-level debug` for verbose output
4. **1Password auth:** Verify `OP_SERVICE_ACCOUNT_TOKEN` is set and valid
