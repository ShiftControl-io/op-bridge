# DaemonStart

Start the op-bridge daemon locally or verify it's already running.

## Steps

### 1. Check if daemon is already running

```bash
~/op-bridge/target/debug/op-bridge ping --socket /tmp/op-bridge.sock 2>/dev/null
```

If this returns "pong", the daemon is already running. Report status and stop.

### 2. Build if binary is missing or stale

Check if the binary exists and is newer than the source:

```bash
ls -la ~/op-bridge/target/debug/op-bridge 2>/dev/null
```

If missing or user requests a fresh build:

```bash
cd ~/op-bridge && cargo build
```

### 3. Determine launch flags

Ask the user or infer from context:

| User intent | Flags |
|-------------|-------|
| Default | `--log-level info` |
| Debug / troubleshoot | `--log-level debug` |
| Deep trace | `--log-level trace` |
| Custom socket | `--socket <path>` |
| File watching | `--watch <spec>` (see WatchConfig workflow) |

### 4. Set up environment

The daemon needs `OP_SERVICE_ACCOUNT_TOKEN` and any `OP_*_REF` env vars. For local development:

```bash
# If using 1Password service account
export OP_SERVICE_ACCOUNT_TOKEN="..."

# Example secret references
export OP_GATEWAY_TOKEN_REF="op://vault/item/field"
```

### 5. Start the daemon

```bash
~/op-bridge/target/debug/op-bridge daemon \
  --socket /tmp/op-bridge.sock \
  --log-level info \
  [ADDITIONAL_FLAGS] &
echo $! > /tmp/op-bridge.pid
```

### 6. Verify

Wait for socket to appear and confirm health:

```bash
while [ ! -S /tmp/op-bridge.sock ]; do sleep 0.1; done
~/op-bridge/target/debug/op-bridge ping
```

Report the PID, socket path, and number of resolved secrets.
