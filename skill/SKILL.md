---
name: OpBridge
description: Manage 1Password secrets via op-bridge daemon — start/stop daemon, read/write/delete secrets, configure file watchers, deploy binary to containers. USE WHEN op-bridge, 1password secrets, secret broker, manage secrets, op bridge daemon, credential sync, watch credentials, deploy op-bridge, container secrets.
---

# OpBridge

Manage the op-bridge 1Password secret broker daemon for OpenClaw or any agent framework that needs secure credential management. Start/stop the daemon, read and write secrets, configure credential file watchers, build and deploy the binary to Docker containers.

## Core Paths

- **Source:** `~/op-bridge/`
- **Binary (dev):** `~/op-bridge/target/debug/op-bridge`
- **Binary (release):** `~/op-bridge/target/release/op-bridge`
- **Default socket:** `/tmp/op-bridge.sock`

## Workflow Routing

| Workflow | Trigger | File |
|----------|---------|------|
| **DaemonStart** | "start op-bridge", "run the daemon" | `Workflows/DaemonStart.md` |
| **DaemonStop** | "stop op-bridge", "kill the daemon" | `Workflows/DaemonStop.md` |
| **SecretOps** | "get secret", "set secret", "delete secret", "list secrets" | `Workflows/SecretOps.md` |
| **Build** | "build op-bridge", "compile", "release build" | `Workflows/Build.md` |
| **Deploy** | "deploy op-bridge", "deploy to container" | `Workflows/Deploy.md` |
| **WatchConfig** | "watch credentials", "configure file watcher", "auto-sync" | `Workflows/WatchConfig.md` |

## Examples

**Example 1: Start the daemon locally for development**
```
User: "Start op-bridge with debug logging"
→ Invokes DaemonStart workflow
→ Builds debug binary if needed
→ Starts daemon with --log-level debug
→ Confirms socket is listening
```

**Example 2: Read a secret from a running daemon**
```
User: "Get the GATEWAY_TOKEN secret from op-bridge"
→ Invokes SecretOps workflow
→ Runs `op-bridge get GATEWAY_TOKEN`
→ Returns the secret value
```

**Example 3: Deploy to containers**
```
User: "Deploy op-bridge to the server"
→ Invokes Deploy workflow
→ Cross-compiles for linux/amd64 (musl)
→ Copies binary to remote host
→ Updates Dockerfile and entrypoint
```

## Quick Reference

### CLI Subcommands

| Command | Description |
|---------|-------------|
| `op-bridge daemon [OPTIONS]` | Run the secret broker daemon |
| `op-bridge get <name>` | Read a secret from the daemon |
| `op-bridge set <name> <uri> <value>` | Write secret to memory + 1Password |
| `op-bridge delete <name>` | Remove secret from memory only |
| `op-bridge refresh` | Re-resolve all secrets from 1Password |
| `op-bridge list` | List all secret reference names |
| `op-bridge ping` | Check if daemon is alive |

### Daemon Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--socket <path>` | `/tmp/op-bridge.sock` | Unix socket path |
| `--prefix <str>` | `OP_` | Env var prefix for secret refs |
| `--suffix <str>` | `_REF` | Env var suffix for secret refs |
| `--log-level <level>` | `info` | error, warn, info, debug, trace |
| `--watch <spec>` | (none) | File watcher spec (repeatable) |

### Watch Spec Format

```
<file-path>=<op://vault/item/field>              # name from filename
<file-path>=<NAME>=<op://vault/item/field>        # explicit name
```

### Socket Protocol

| Command | Response |
|---------|----------|
| `PING` | `OK pong` |
| `LIST` | `OK name1,name2,...` |
| `GET <name>` | `OK <value>` or `ERR unknown ref: <name>` |
| `SET <name> <uri> <value>` | `OK` or `ERR <message>` |
| `DELETE <name>` | `OK` or `ERR unknown ref: <name>` |

### Signals

| Signal | Effect |
|--------|--------|
| `SIGHUP` | Re-resolve all secrets from 1Password |
| `SIGTERM` / `SIGINT` | Graceful shutdown (zeroize + cleanup) |

### Environment Variables

- `OP_SERVICE_ACCOUNT_TOKEN` — Required by `op` CLI
- `OP_{NAME}_REF` — Secret references (e.g., `OP_GATEWAY_TOKEN_REF=op://vault/item/field`)
- `RUST_LOG` — Overrides `--log-level` flag (e.g., `RUST_LOG=op_bridge=trace`)

## Security Notes

- Secrets are **never written to disk** by op-bridge
- All values use `SecretString` (mlock'd, zeroized on drop)
- `DELETE` removes from memory only — op-bridge **cannot delete 1Password items**
- `SET` can update existing 1Password fields but cannot create or delete items
- Unix socket is local-only (no network exposure)
