//! Unix domain socket server implementing the op-bridge wire protocol.
//!
//! The protocol is line-based and newline-delimited. Each request is a single
//! line; each response is a single line prefixed with `OK` or `ERR`. Commands
//! are case-insensitive.
//!
//! ## Protocol reference
//!
//! | Command | Response |
//! |---------|----------|
//! | `PING` | `OK pong` |
//! | `LIST` | `OK name1,name2,...` |
//! | `GET <name>` | `OK <value>` or `ERR unknown ref: <name>` |
//! | `SET <name> <op://uri> <value>` | `OK` or `ERR <message>` |
//! | `DELETE <name>` | `OK` or `ERR unknown ref: <name>` |

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tracing::{debug, error, info, trace};

use crate::{resolver, store::SecretStore};

/// Binds a [`UnixListener`] at the given path, removing any stale socket file.
///
/// If a socket file already exists at `path` (e.g., from a previous unclean
/// shutdown), it is removed before binding. This avoids "address already in use"
/// errors on restart.
///
/// The socket is created with `0600` permissions (owner read/write only),
/// preventing other local users from connecting and reading secrets.
pub async fn bind(path: &Path) -> std::io::Result<UnixListener> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let listener = UnixListener::bind(path)?;
    // Restrict socket to owner-only access (prevents other local users from reading secrets)
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Handles a single client connection, processing commands until EOF.
///
/// Each connection is fully independent — the client sends one or more
/// newline-terminated commands, and each receives a single-line response.
/// The connection is closed when the client disconnects (EOF on the read half).
///
/// # Concurrency
///
/// Read operations (`GET`, `LIST`, `PING`) acquire a read lock on the store,
/// allowing concurrent readers. Write operations (`SET`, `DELETE`) acquire a
/// write lock, blocking other access for the duration of the operation.
pub async fn handle_client(
    stream: UnixStream,
    store: &Arc<RwLock<SecretStore>>,
) -> std::io::Result<()> {
    debug!("client connected");
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(raw) = lines.next_line().await? {
        let line = raw.trim();
        // Log command name at trace level — never log full line (SET contains secrets)
        trace!(
            cmd = line.split_whitespace().next().unwrap_or(""),
            "raw request"
        );
        if line.is_empty() {
            continue;
        }

        let upper = line.to_ascii_uppercase();
        let response = if upper == "PING" {
            "OK pong\n".to_string()
        } else if upper == "LIST" {
            let s = store.read().await;
            let keys = s.keys();
            format!("OK {}\n", keys.join(","))
        } else if let Some(rest) = upper.strip_prefix("GET ") {
            let ref_name = rest.trim();
            let s = store.read().await;
            match s.get(ref_name) {
                Some(value) => format!("OK {value}\n"),
                None => format!("ERR unknown ref: {ref_name}\n"),
            }
        } else if upper.starts_with("SET ") {
            handle_set(line, store).await
        } else if let Some(rest) = upper.strip_prefix("DELETE ") {
            let ref_name = rest.trim();
            let mut s = store.write().await;
            if s.remove(ref_name) {
                info!("DELETE {ref_name} (removed from memory, NOT from 1Password)");
                "OK\n".to_string()
            } else {
                format!("ERR unknown ref: {ref_name}\n")
            }
        } else {
            format!("ERR unknown command: {line}\n")
        };

        // Log command name only — never log response content (may contain secrets)
        debug!(
            cmd = line.split_whitespace().next().unwrap_or(""),
            status = if response.starts_with("OK") {
                "OK"
            } else {
                "ERR"
            },
            "request handled"
        );
        writer.write_all(response.as_bytes()).await?;
    }

    Ok(())
}

/// Processes a `SET <name> <op://uri> <value>` command.
///
/// The original (non-uppercased) line is used to preserve the case of the
/// secret value. The operation writes to 1Password first via [`resolver::op_write`],
/// and only updates the in-memory store on success (fail-fast strategy).
async fn handle_set(line: &str, store: &Arc<RwLock<SecretStore>>) -> String {
    let rest = line[4..].trim();

    let mut parts = rest.splitn(3, ' ');
    let name = match parts.next() {
        Some(n) if !n.is_empty() => n.to_ascii_uppercase(),
        _ => return "ERR SET requires: <name> <op://uri> <value>\n".to_string(),
    };
    let uri = match parts.next() {
        Some(u) if u.starts_with("op://") => u,
        _ => return "ERR SET requires valid op:// URI as second argument\n".to_string(),
    };
    let value = match parts.next() {
        Some(v) if !v.is_empty() => v,
        _ => return "ERR SET requires a value as third argument\n".to_string(),
    };

    if let Err(e) = resolver::op_write(uri, value).await {
        error!("SET write-back failed for {}: {e}", name);
        return format!("ERR write-back failed: {e}\n");
    }

    {
        let mut s = store.write().await;
        s.insert_with_uri(
            name.clone(),
            secrecy::SecretString::from(value.to_string()),
            uri.to_string(),
        );
    }

    info!("SET {} -> {uri}", name);
    "OK\n".to_string()
}
