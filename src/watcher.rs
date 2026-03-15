//! File system watcher for automatic credential write-back to 1Password.
//!
//! When enabled via `--watch` flags on the daemon, this module monitors
//! specified files for changes and automatically writes their contents back
//! to 1Password. Designed for agent containers where OAuth tokens or other
//! credentials get refreshed at runtime and need to be persisted.
//!
//! ## Watch spec format
//!
//! ```text
//! /path/to/file=op://vault/item/field           # name derived from filename
//! /path/to/file=MY_NAME=op://vault/item/field    # explicit name
//! ```
//!
//! The watcher monitors the **parent directory** of each file (not the file
//! itself), so it works even if the file doesn't exist yet or is recreated
//! by atomic-write patterns (write temp + rename).

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use secrecy::SecretString;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::{resolver, store::SecretStore};

/// A mapping from a file path to a reference name and 1Password URI.
///
/// Created by [`parse_watch_spec`] from CLI `--watch` arguments.
#[derive(Debug, Clone)]
pub struct WatchEntry {
    /// The file path to monitor for changes.
    pub path: PathBuf,
    /// The reference name for the in-memory store (e.g., `"CLAUDE_CREDS"`).
    pub name: String,
    /// The 1Password `op://` URI to write the file contents to on change.
    pub uri: String,
}

/// Parses a `--watch` CLI argument into a [`WatchEntry`].
///
/// Accepts two formats:
///
/// - `<path>=<op://uri>` — derives the name from the filename by uppercasing
///   and replacing dots with underscores (e.g., `creds.json` → `CREDS_JSON`).
/// - `<path>=<name>=<op://uri>` — uses the explicitly provided name.
///
/// # Errors
///
/// Returns a descriptive error if the spec doesn't match either format or if
/// the URI doesn't start with `op://`.
pub fn parse_watch_spec(spec: &str) -> Result<WatchEntry, String> {
    let parts: Vec<&str> = spec.splitn(3, '=').collect();
    match parts.len() {
        2 if parts[1].starts_with("op://") => {
            let path = PathBuf::from(parts[0]);
            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .map(|f| f.replace('.', "_").to_ascii_uppercase())
                .ok_or_else(|| format!("cannot derive name from path: {}", parts[0]))?;
            Ok(WatchEntry {
                path,
                name,
                uri: parts[1].to_string(),
            })
        }
        3 if parts[2].starts_with("op://") => Ok(WatchEntry {
            path: PathBuf::from(parts[0]),
            name: parts[1].to_string(),
            uri: parts[2].to_string(),
        }),
        _ => Err(format!(
            "invalid watch spec: {spec}\n  expected: <path>=<op://uri> or <path>=<name>=<op://uri>"
        )),
    }
}

/// Starts file system watchers for the given entries.
///
/// Spawns a background tokio task that receives file system events from the
/// [`notify`] crate and, for each matching modify/create event:
///
/// 1. Reads the file contents.
/// 2. Writes the value to 1Password via [`resolver::op_write`].
/// 3. Updates the in-memory [`SecretStore`].
///
/// # Returns
///
/// A [`RecommendedWatcher`] handle that **must be kept alive** for the duration
/// of the daemon. Dropping it stops all file watches.
///
/// # Errors
///
/// Returns an error if the watcher cannot be created or if any parent directory
/// cannot be watched.
pub async fn start_watchers(
    entries: Vec<WatchEntry>,
    store: Arc<RwLock<SecretStore>>,
) -> Result<RecommendedWatcher, String> {
    if entries.is_empty() {
        return Err("no watch entries provided".to_string());
    }

    // Build path → entry lookup using canonical paths for reliable matching.
    let path_map: HashMap<PathBuf, WatchEntry> = entries
        .iter()
        .map(|e| {
            let canonical = e.path.canonicalize().unwrap_or_else(|_| e.path.clone());
            (canonical, e.clone())
        })
        .collect();

    let (tx, mut rx) = mpsc::channel::<Event>(64);

    // Background task: process file change events and write back to 1Password.
    let store_for_handler = Arc::clone(&store);
    let path_map_for_handler = path_map.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                continue;
            }

            for path in &event.paths {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if let Some(entry) = path_map_for_handler.get(&canonical) {
                    debug!(
                        "file changed: {} -> writing back to {}",
                        path.display(),
                        entry.uri
                    );
                    // Check file size before reading (prevent OOM from large files)
                    const MAX_FILE_SIZE: u64 = 1_048_576; // 1 MB
                    match tokio::fs::metadata(path).await {
                        Ok(meta) if meta.len() > MAX_FILE_SIZE => {
                            error!(
                                "file {} is too large ({} bytes, max {}), skipping write-back",
                                path.display(),
                                meta.len(),
                                MAX_FILE_SIZE
                            );
                            continue;
                        }
                        Err(e) => {
                            error!("failed to stat {}: {}", path.display(), e);
                            continue;
                        }
                        _ => {}
                    }

                    match tokio::fs::read_to_string(path).await {
                        Ok(contents) => {
                            let value = contents.trim().to_string();
                            if value.is_empty() {
                                warn!("file {} is empty, skipping write-back", path.display());
                                continue;
                            }

                            if let Err(e) = resolver::op_write(&entry.uri, &value).await {
                                error!(
                                    "write-back failed for {} -> {}: {}",
                                    entry.name, entry.uri, e
                                );
                                continue;
                            }

                            {
                                let mut s = store_for_handler.write().await;
                                s.insert_with_uri(
                                    entry.name.clone(),
                                    SecretString::from(value),
                                    entry.uri.clone(),
                                );
                            }

                            info!("write-back completed: {} -> {}", entry.name, entry.uri);
                        }
                        Err(e) => {
                            error!("failed to read {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    });

    // Create the platform-specific file system watcher.
    let tx_for_watcher = tx;
    let mut watcher = RecommendedWatcher::new(
        move |result: Result<Event, notify::Error>| match result {
            Ok(event) => {
                let _ = tx_for_watcher.blocking_send(event);
            }
            Err(e) => {
                error!("watch error: {e}");
            }
        },
        notify::Config::default(),
    )
    .map_err(|e| format!("failed to create watcher: {e}"))?;

    // Watch the parent directory of each file (handles atomic-write patterns).
    for entry in &entries {
        let watch_path = entry.path.parent().unwrap_or_else(|| Path::new("."));
        watcher
            .watch(watch_path, RecursiveMode::NonRecursive)
            .map_err(|e| format!("failed to watch {}: {e}", watch_path.display()))?;
        info!(
            "watching {} (name={}, uri={})",
            entry.path.display(),
            entry.name,
            entry.uri
        );
    }

    Ok(watcher)
}
