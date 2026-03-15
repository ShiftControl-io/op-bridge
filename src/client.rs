//! Unix socket client for communicating with the op-bridge daemon.
//!
//! Each function connects to the daemon socket, sends a single command, reads
//! the response, and disconnects. Connections are short-lived by design — there
//! is no persistent session or multiplexing.
//!
//! Used by the CLI subcommands (`get`, `set`, `list`, `delete`, `ping`).

use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Sends a single command to the daemon and returns the raw response line.
///
/// Opens a new connection, writes the command followed by a newline, shuts down
/// the write half (signaling end of request), and reads the first response line.
async fn send_command(socket: &Path, command: &str) -> Result<String, String> {
    let stream = UnixStream::connect(socket)
        .await
        .map_err(|e| format!("failed to connect to {}: {e}", socket.display()))?;

    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(format!("{command}\n").as_bytes())
        .await
        .map_err(|e| format!("write failed: {e}"))?;
    writer
        .shutdown()
        .await
        .map_err(|e| format!("shutdown failed: {e}"))?;

    let mut lines = BufReader::new(reader).lines();
    lines
        .next_line()
        .await
        .map_err(|e| format!("read failed: {e}"))?
        .ok_or_else(|| "no response from daemon".to_string())
}

/// Parses a daemon response into `Ok(payload)` or `Err(message)`.
///
/// Responses prefixed with `"OK"` return the payload (everything after `"OK "`).
/// Responses prefixed with `"ERR "` return the error message. Anything else is
/// treated as an unexpected response error.
fn parse_response(response: &str) -> Result<String, String> {
    if let Some(value) = response.strip_prefix("OK") {
        Ok(value.trim_start().to_string())
    } else if let Some(err) = response.strip_prefix("ERR ") {
        Err(err.to_string())
    } else {
        Err(format!("unexpected response: {response}"))
    }
}

/// Checks if the daemon is alive by sending a `PING` command.
///
/// Returns `Ok(())` if the daemon responds with `OK pong`, or an error if the
/// socket is unreachable or the daemon responds unexpectedly.
pub async fn ping(socket: &Path) -> Result<(), String> {
    let resp = send_command(socket, "PING").await?;
    parse_response(&resp).map(|_| ())
}

/// Retrieves a secret value from the daemon by reference name.
///
/// Returns the secret as a plain `String`. The caller is responsible for
/// handling the value securely (avoid logging, limit lifetime, etc.).
pub async fn get(socket: &Path, name: &str) -> Result<String, String> {
    let resp = send_command(socket, &format!("GET {name}")).await?;
    parse_response(&resp)
}

/// Lists all secret reference names currently held by the daemon.
///
/// Returns an empty `Vec` if no secrets are loaded. Names are returned in
/// arbitrary order (HashMap iteration order on the server side).
pub async fn list(socket: &Path) -> Result<Vec<String>, String> {
    let resp = send_command(socket, "LIST").await?;
    let names = parse_response(&resp)?;
    if names.is_empty() {
        Ok(Vec::new())
    } else {
        Ok(names.split(',').map(|s| s.to_string()).collect())
    }
}

/// Sets a secret value, updating both the in-memory store and 1Password.
///
/// The daemon writes to 1Password first via `op item edit`. If the write-back
/// fails, the in-memory store is **not** updated (fail-fast).
///
/// # Arguments
///
/// * `name` — The reference name to store the secret under.
/// * `uri` — The 1Password `op://vault/item/field` URI to write to.
/// * `value` — The new secret value.
pub async fn set(socket: &Path, name: &str, uri: &str, value: &str) -> Result<(), String> {
    let resp = send_command(socket, &format!("SET {name} {uri} {value}")).await?;
    parse_response(&resp).map(|_| ())
}

/// Removes a secret from the daemon's in-memory store.
///
/// This does **not** delete anything from 1Password — it only clears the
/// secret from memory (with zeroization). Useful for revoking access to a
/// secret without affecting the upstream source of truth.
pub async fn delete(socket: &Path, name: &str) -> Result<(), String> {
    let resp = send_command(socket, &format!("DELETE {name}")).await?;
    parse_response(&resp).map(|_| ())
}
