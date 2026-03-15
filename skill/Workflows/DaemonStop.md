# DaemonStop

Gracefully stop the op-bridge daemon.

## Steps

### 1. Check if daemon is running

```bash
~/op-bridge/target/debug/op-bridge ping --socket /tmp/op-bridge.sock 2>/dev/null
```

If not reachable, report that no daemon is running and stop.

### 2. Send SIGTERM for graceful shutdown

```bash
kill $(cat /tmp/op-bridge.pid 2>/dev/null) 2>/dev/null
```

If PID file doesn't exist, find the process:

```bash
pgrep -f "op-bridge daemon"
```

### 3. Verify shutdown

Wait briefly and confirm the socket is gone:

```bash
sleep 1
test ! -S /tmp/op-bridge.sock && echo "Daemon stopped" || echo "Socket still exists"
```

### 4. Clean up

Remove stale PID file if present:

```bash
rm -f /tmp/op-bridge.pid
```

Report that all secrets have been zeroized from memory and the socket has been removed.
