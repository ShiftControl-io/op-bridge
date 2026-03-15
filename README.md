# op-bridge

[![CI](https://github.com/ShiftControl-io/op-bridge/actions/workflows/ci.yml/badge.svg)](https://github.com/ShiftControl-io/op-bridge/actions/workflows/ci.yml)
[![Test Coverage](https://img.shields.io/endpoint?url=https://gist.githubusercontent.com/shiftcontrol-dan/efc94b81793e102c4e4318580bda039e/raw/op_bridge_coverage.json)](#)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Lightweight 1Password secret broker daemon for Docker containers. Resolves secrets once at startup, holds them in memory (mlock'd, zeroized on drop), and serves them via Unix socket. Supports read and write-back to 1Password, plus file watching for credential auto-sync.

## Why

### The problem with cleartext credentials

Without a secret manager, AI agent containers store API keys, OAuth tokens, and service credentials as plaintext environment variables or `.env` files. This means secrets are visible in `docker inspect`, process listings, shell history, and container images. If a container is compromised, every credential is immediately exposed.

### OpenClaw's SecretRef — the right direction

[OpenClaw](https://github.com/openclaw/openclaw) solves this with **SecretRef**, a named credential reference system. Instead of passing `ANTHROPIC_API_KEY=sk-ant-...` as an environment variable, you configure a SecretRef that resolves the credential at runtime from an external secret manager:

```json5
{
  secrets: {
    providers: {
      anthropic_key: {
        source: "exec",
        command: "/usr/local/bin/op",
        args: ["read", "op://Bot-Dan/Anthropic-Key/password"],
      },
    },
  },
}
```

The **exec provider** calls an external binary (like 1Password's `op` CLI) at startup, resolves all secrets eagerly into an in-memory snapshot, and never persists them to disk. Agents never see the raw credentials — they work through opaque references.

### Where SecretRef falls short with 1Password

While SecretRef's exec provider pattern is sound, using `op` CLI directly has real limitations:

- **Rate limits.** Each `op read` call hits 1Password's API. Busy agents that resolve secrets frequently, multi-agent deployments sharing a single 1Password organization, or fleet restarts can easily trigger 1Password's burst throttling (~50 rapid calls) and daily limits (10K/day per service account). The more agents you run, the faster you hit the ceiling.
- **Read-only.** The exec provider resolves secrets at startup but has no mechanism to write back. This is fatal for OAuth flows: when a tool refreshes its access token, it gets a new single-use refresh token and the old one is invalidated. The new token only exists in the container's local filesystem. If the container restarts, the exec provider re-reads the *old* (now-invalid) refresh token from 1Password — breaking authentication permanently until someone manually re-authenticates.
- **One provider per secret.** SecretRef IDs can't contain spaces, so each 1Password reference needs its own provider definition. With many secrets, configuration becomes verbose.
- **Cold start cost.** Every container restart re-resolves every secret through `op`, adding latency and API load.

### How op-bridge fixes this

op-bridge sits between OpenClaw's exec provider and 1Password, acting as a local caching daemon with write-back:

```
OpenClaw SecretRef ──exec──→ op-bridge get ──socket──→ op-bridge daemon ──cache──→ 1Password
                                                              ↑
                              credential file change ─watch──→│──write-back──→ 1Password
```

| Problem | op-bridge solution |
|---------|-------------------|
| Rate limits from repeated `op read` | Resolves all secrets once at startup, serves from memory. SIGHUP to refresh. |
| No write-back for rotated credentials | `SET` command and file watcher write updated values back to 1Password via `op item edit`. Entire credential files (access + refresh tokens) are persisted atomically. |
| Lost tokens on container restart | File watcher detects credential file changes and syncs to 1Password immediately. Next restart reads the fresh tokens — no manual re-auth needed. |
| Verbose per-secret provider config | Daemon auto-discovers all `OP_*_REF` env vars at startup — no per-secret configuration needed. One exec provider pointing to `op-bridge get` serves any secret by name. |
| Cold start latency | Sub-millisecond reads from Unix socket vs. 100ms+ per `op read` API call. |

**op-bridge doesn't modify SecretRef.** It works as a drop-in replacement for the `op` binary in the exec provider's `command` field. OpenClaw's configuration stays the same — you just point the exec command at `op-bridge get` instead of `op read`.

## Architecture

```
entrypoint.sh
  ├── starts op-bridge daemon (background)
  ├── waits for socket ready
  └── exec application

op-bridge daemon
  ├── discovers OP_*_REF env vars
  ├── resolves each via `op read`
  ├── stores in SecretString (zeroize-on-drop)
  ├── serves via Unix socket (GET/SET/LIST/PING)
  └── optionally watches files for credential changes
```

## CLI

op-bridge is both a daemon and a CLI client — a single binary for everything.

```
op-bridge daemon              # run the secret broker daemon
op-bridge get <name>          # read a secret from the daemon
op-bridge set <name> <uri> <value>  # write a secret to 1Password + memory
op-bridge delete <name>       # remove a secret from memory (not from 1Password)
op-bridge refresh             # re-resolve all secrets from 1Password
op-bridge list                # list all secret reference names
op-bridge ping                # check if the daemon is running
```

All client commands accept `--socket <path>` (default: `/tmp/op-bridge.sock`).

## Usage

### Automatic Secret Discovery

On startup, the daemon scans all environment variables for the pattern `OP_{NAME}_REF` where the value starts with `op://`. Each matching variable becomes a secret that is automatically resolved from 1Password and served by name.

For example, with these environment variables set:

```bash
export OP_SERVICE_ACCOUNT_TOKEN="..."           # Required by `op` CLI (not a ref — used for auth)
export OP_GATEWAY_TOKEN_REF="op://vault/item/field"
export OP_SLACK_BOT_TOKEN_REF="op://vault/item2/field"
export OP_ANTHROPIC_KEY_REF="op://vault/item3/field"
```

The daemon discovers three refs, strips the `OP_` prefix and `_REF` suffix, resolves each via `op read`, and serves them by canonical name:

| Env var | Canonical name | Resolved from |
|---------|---------------|---------------|
| `OP_GATEWAY_TOKEN_REF` | `GATEWAY_TOKEN` | `op://vault/item/field` |
| `OP_SLACK_BOT_TOKEN_REF` | `SLACK_BOT_TOKEN` | `op://vault/item2/field` |
| `OP_ANTHROPIC_KEY_REF` | `ANTHROPIC_KEY` | `op://vault/item3/field` |

You can then read any of these with `op-bridge get GATEWAY_TOKEN`. The prefix and suffix are configurable via `--prefix` and `--suffix` flags if your naming convention differs.

### Start the Daemon

```bash
op-bridge daemon --socket /tmp/op-bridge.sock
```

### Read Secrets

```bash
op-bridge get GATEWAY_TOKEN
# prints the secret value to stdout (no trailing newline)
```

### Write Secrets

```bash
op-bridge set OAUTH_TOKEN op://vault/item/token "new-token-value"
# updates in-memory store AND writes back to 1Password via `op item edit`
```

### File Watching and OAuth Token Persistence

Many tools inside agent containers use OAuth for authentication. OAuth flows produce two tokens:

- **Access token** — short-lived (minutes to hours), used for API calls
- **Refresh token** — long-lived, used to obtain new access+refresh token pairs when the access token expires

The critical detail: **refresh tokens are often single-use**. When a tool refreshes its access token, it receives a *new* refresh token and the old one is invalidated. The tool writes both tokens to a local credentials file.

**The problem without op-bridge:**

```
1. Container starts → reads refresh token from 1Password (token_v1)
2. Tool authenticates → gets access_token + refresh token_v2
3. Tool writes token_v2 to /app/credentials.json
4. Container crashes or restarts
5. Container reads token_v1 from 1Password (stale — already invalidated)
6. Authentication fails permanently — manual re-auth required
```

**With op-bridge file watching:**

```
1. Container starts → op-bridge resolves token_v1 from 1Password
2. Tool authenticates → gets access_token + refresh token_v2
3. Tool writes token_v2 to /app/credentials.json
4. op-bridge detects file change → writes token_v2 back to 1Password
5. Container crashes or restarts
6. op-bridge resolves token_v2 from 1Password (fresh!)
7. Authentication succeeds
```

The file watcher treats the credentials file as an opaque blob — it reads the entire file contents and stores them as a single 1Password field value. This works naturally with JSON credential files that contain both access and refresh tokens:

```json
{
  "access_token": "eyJhbG...",
  "refresh_token": "dGhpcyBpcyBhIHJlZnJlc2ggdG9rZW4...",
  "expires_at": "2026-03-15T16:00:00Z"
}
```

The entire JSON is persisted atomically to 1Password on every change, so both tokens are always in sync.

#### Configuration

```bash
op-bridge daemon \
  --watch /app/credentials.json=OAUTH_CREDS=op://vault/My-Agent/oauth-credentials \
  --watch /app/api-token.txt=API_TOKEN=op://vault/Service/api-key
```

Format: `<file-path>=<op://uri>` or `<file-path>=<name>=<op://uri>`

Multiple `--watch` flags can be specified. Each watches a different file and writes changes to its own 1Password field.

#### How it works

- The watcher monitors the **parent directory** of each file (not the file itself), so it handles atomic-write patterns (write temp file + rename) used by most credential libraries
- On modify/create events, the file contents are read, trimmed, and written to 1Password via `op item edit`
- The in-memory store is also updated with the new value
- Empty files are skipped (logged as a warning)

### Protocol

Line-based, newline-delimited (for direct socket use):

| Request | Response |
|---------|----------|
| `PING` | `OK pong` |
| `GET <name>` | `OK <value>` or `ERR unknown ref: <name>` |
| `SET <name> <op://uri> <value>` | `OK` or `ERR <message>` |
| `DELETE <name>` | `OK` or `ERR unknown ref: <name>` |
| `LIST` | `OK <name1>,<name2>,...` |

Commands are case-insensitive.

### Refresh Secrets

Re-resolve all secrets from 1Password (useful after rotating credentials upstream):

```bash
op-bridge refresh
```

This reads the daemon's PID file and sends SIGHUP. You can also send the signal directly:

```bash
kill -HUP $(cat /tmp/op-bridge.pid)
```

## Integration with OpenClaw

### 1. Add the binary to your Dockerfile

```dockerfile
COPY op-bridge /usr/local/bin/op-bridge
RUN chown 1000:1000 /usr/local/bin/op-bridge && chmod 755 /usr/local/bin/op-bridge
```

The binary must be owned by the container user (uid 1000 for the `node` user in OpenClaw images). OpenClaw's exec provider validates binary ownership as a security check.

### 2. Update entrypoint.sh

Replace direct `op run` / `op read` calls with op-bridge:

```bash
#!/bin/sh
set -e

# Start op-bridge daemon (resolves all OP_*_REF env vars from 1Password)
# Add --watch flags for any credential files that rotate at runtime
op-bridge daemon \
  --log-level info \
  --watch /app/credentials.json=OAUTH_CREDS=op://vault/OAuth-Token/credentials \
  &
echo $! > /tmp/op-bridge.pid

# Wait for socket
while [ ! -S /tmp/op-bridge.sock ]; do sleep 0.1; done

# Export resolved secrets as env vars
for ref_name in $(op-bridge list); do
  value=$(op-bridge get "$ref_name")
  export "$ref_name=$value"
done

exec openclaw gateway "$@"
```

### 3. Point SecretRef exec providers at op-bridge (optional)

For secrets resolved through OpenClaw's SecretRef system (not environment variables), point the exec provider at op-bridge instead of `op`:

```json5
// Before: calls op directly (slow, rate-limited, no write-back)
{
  secrets: {
    providers: {
      anthropic: {
        source: "exec",
        command: "/usr/local/bin/op",
        args: ["read", "--no-newline", "op://Bot-Dan/Anthropic-Key/password"],
      },
    },
  },
}

// After: calls op-bridge (cached, sub-millisecond, write-back capable)
{
  secrets: {
    providers: {
      anthropic: {
        source: "exec",
        command: "/usr/local/bin/op-bridge",
        args: ["get", "ANTHROPIC_KEY"],
      },
    },
  },
}
```

The daemon must be running before OpenClaw starts (the entrypoint handles this). Secrets are already resolved and cached — the exec provider just reads from the local Unix socket.

## Building

### Development (macOS)

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo audit
```

### Release (Linux static binary)

```bash
rustup target add x86_64-unknown-linux-musl
cargo build --release --target x86_64-unknown-linux-musl
```

## Agent Skill

This repo includes an agent skill in `skill/` with structured workflows for managing op-bridge. Compatible with any AI agent framework that supports skill-based routing.

### Install

```bash
# Symlink into your agent's skills directory
ln -s "$(pwd)/skill" ~/.claude/skills/OpBridge
```

The skill provides workflows for: starting/stopping the daemon, reading/writing/deleting secrets, building, deploying to containers, and configuring file watchers.

## Security

- Secrets are never written to disk by op-bridge
- All secret values use `SecretString` from the `secrecy` crate (mlock'd memory, zeroized on drop)
- Secrets are zeroized on graceful shutdown (SIGTERM/SIGINT)
- Unix socket is local-only (no network exposure)
- Write-back uses the same security boundary as `op` CLI itself
- The daemon runs as non-root (same user as the container process)

## License

MIT
