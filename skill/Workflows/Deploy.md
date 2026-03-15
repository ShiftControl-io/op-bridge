# Deploy

Build and deploy op-bridge to Docker containers on remote hosts.

## Steps

### 1. Determine target

Ask the user for:
- **Host** — the remote server (e.g., `myserver.example.com`)
- **Path** — the deploy directory on that host (e.g., `~/openclaw/`)
- **Architecture** — typically `x86_64` (amd64) or `aarch64` (arm64)

### 2. Build release binary for the target architecture

```bash
cd ~/op-bridge
```

| Architecture | Target | Command |
|-------------|--------|---------|
| x86_64 (amd64) | `x86_64-unknown-linux-musl` | `cargo build --release --target x86_64-unknown-linux-musl` |
| aarch64 (arm64) | `aarch64-unknown-linux-musl` | `cargo build --release --target aarch64-unknown-linux-musl` |

Ensure the target is installed:

```bash
rustup target add <TARGET>
```

Verify the binary is statically linked:

```bash
file target/<TARGET>/release/op-bridge
```

Should say "statically linked" or "static-pie linked".

### 3. Copy binary to remote host

```bash
scp target/<TARGET>/release/op-bridge <HOST>:<PATH>/op-bridge
```

### 4. Update Dockerfile

Add op-bridge to the container Dockerfile if not already present:

```dockerfile
COPY op-bridge /usr/local/bin/op-bridge
RUN chown 1000:1000 /usr/local/bin/op-bridge && chmod 755 /usr/local/bin/op-bridge
```

The binary must be owned by the container user. OpenClaw's exec provider validates binary ownership as a security check.

### 5. Update entrypoint.sh

Replace direct `op read` / `op run` calls with op-bridge:

```bash
# Start op-bridge daemon
op-bridge daemon \
  --socket /tmp/op-bridge.sock \
  --log-level info \
  [--watch SPECS IF NEEDED] &
echo $! > /tmp/op-bridge.pid

# Wait for socket
while [ ! -S /tmp/op-bridge.sock ]; do sleep 0.1; done

# Export resolved secrets as env vars
for ref_name in $(op-bridge list); do
  value=$(op-bridge get "$ref_name")
  export "$ref_name=$value"
done
```

### 6. Rebuild and restart containers

```bash
ssh <HOST> << 'REMOTEOF'
cd <PATH>
docker compose build --no-cache
docker compose up -d
REMOTEOF
```

### 7. Verify deployment

Check that containers are healthy:

```bash
ssh <HOST> "docker ps --format '{{.Names}}\t{{.Status}}'"
```

Check op-bridge logs inside a container:

```bash
ssh <HOST> "docker logs <CONTAINER_NAME> 2>&1 | grep op-bridge | head -10"
```
