# WatchConfig

Configure file watchers for automatic credential write-back to 1Password.

## Overview

File watching is designed for containers where credential files (e.g., OAuth tokens, API keys) get refreshed at runtime and need to be persisted back to 1Password automatically. This is common in OpenClaw and similar agent frameworks where tools rotate credentials during operation.

## Steps

### 1. Identify files to watch

Ask the user which credential files need monitoring. Common patterns:

| File | Purpose | Watch spec example |
|------|---------|-------------------|
| OAuth token | Token refresh persistence | `/app/credentials.json=OAUTH_CREDS=op://vault/OAuth-Token/credentials` |
| API key rotation | Key rotation sync | `/app/api-key.txt=API_KEY=op://vault/Service-Key/key` |
| Custom token file | Application token rotation | `/app/token.json=APP_TOKEN=op://vault/item/field` |

### 2. Construct --watch flags

Each watched file needs a `--watch` flag on the daemon command:

**Auto-derived name (from filename):**
```
--watch /path/to/creds.json=op://vault/item/field
```
Name becomes `CREDS_JSON` (filename uppercased, dots→underscores).

**Explicit name:**
```
--watch /path/to/creds.json=MY_CREDS=op://vault/item/field
```

### 3. Test the watcher

Start the daemon with watch flags and verify:

```bash
op-bridge daemon \
  --log-level debug \
  --watch /tmp/test-creds.txt=TEST_CREDS=op://vault/item/field &

# Wait for socket
while [ ! -S /tmp/op-bridge.sock ]; do sleep 0.1; done

# Modify the watched file
echo "new-value" > /tmp/test-creds.txt

# Check daemon logs for write-back confirmation
# Should see: "file changed: /tmp/test-creds.txt -> writing back to op://..."
# And: "write-back completed: TEST_CREDS -> op://..."
```

### 4. Integrate into entrypoint

Add the `--watch` flags to the daemon startup in the container entrypoint:

```bash
op-bridge daemon \
  --socket /tmp/op-bridge.sock \
  --log-level info \
  --watch /app/credentials.json=OAUTH_CREDS=op://vault/OAuth-Token/credentials \
  &
```

## How It Works

- The watcher monitors the **parent directory** of each file (not the file itself)
- This handles atomic-write patterns (write temp file + rename)
- On modify/create events, the file contents are read, trimmed, and written to 1Password via `op item edit`
- The in-memory store is also updated with the new value
- Empty files are skipped (logged as warning)

## Troubleshooting

- **Watcher not firing:** Check that the parent directory exists. The watcher can't watch a non-existent directory.
- **Write-back failing:** Run with `--log-level debug` to see the `op item edit` command and error output.
- **Too many events:** Some editors trigger multiple events per save. The watcher deduplicates by checking file content, but you may see multiple log entries.
