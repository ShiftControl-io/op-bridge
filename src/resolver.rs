//! 1Password CLI integration for reading and writing secrets.
//!
//! This module wraps the `op` CLI binary to resolve `op://` references into
//! secret values ([`op_read`]) and to write updated values back to 1Password
//! ([`op_write`]). All operations spawn `op` as a child process, so the
//! `OP_SERVICE_ACCOUNT_TOKEN` environment variable must be set.

use secrecy::SecretString;
use tracing::{debug, error, info, trace};

use crate::store::SecretStore;

/// A discovered secret reference mapping an environment variable name to an
/// `op://` URI.
///
/// Created by [`discover_refs`] from environment variables matching the
/// `{prefix}*{suffix}` pattern (e.g., `OP_GATEWAY_TOKEN_REF`).
#[derive(Debug, Clone)]
pub struct SecretRef {
    /// The canonical name derived from the env var (e.g., `"GATEWAY_TOKEN"`).
    pub name: String,
    /// The 1Password reference URI (e.g., `"op://vault/item/field"`).
    pub uri: String,
}

/// Scans environment variables for secret references matching the given
/// prefix and suffix pattern.
///
/// For each env var where:
/// - the key starts with `prefix` and ends with `suffix`, and
/// - the value starts with `op://`,
///
/// a [`SecretRef`] is created with the name derived by stripping the prefix
/// and suffix. For example, with `prefix="OP_"` and `suffix="_REF"`:
///
/// ```text
/// OP_GATEWAY_TOKEN_REF="op://vault/item/field"  →  name="GATEWAY_TOKEN"
/// ```
///
/// Results are sorted alphabetically by name for deterministic ordering.
pub fn discover_refs(prefix: &str, suffix: &str) -> Vec<SecretRef> {
    let mut refs = Vec::new();

    for (key, value) in std::env::vars() {
        if key.starts_with(prefix) && key.ends_with(suffix) && value.starts_with("op://") {
            let name = &key[prefix.len()..key.len() - suffix.len()];
            if !name.is_empty() {
                info!("found ref: {} -> {}", name, value);
                refs.push(SecretRef {
                    name: name.to_string(),
                    uri: value,
                });
            }
        }
    }

    refs.sort_by(|a, b| a.name.cmp(&b.name));
    refs
}

/// Resolves all secret references via `op read` and inserts them into the store.
///
/// Each reference is resolved sequentially. Failures are logged at `error` level
/// but do not abort resolution of remaining references.
///
/// # Returns
///
/// A tuple of `(success_count, failure_count)`.
pub async fn resolve_all(refs: &[SecretRef], store: &mut SecretStore) -> (usize, usize) {
    let mut ok = 0;
    let mut fail = 0;

    for r in refs {
        match op_read(&r.uri).await {
            Ok(value) => {
                store.insert_with_uri(r.name.clone(), value, r.uri.clone());
                info!(
                    "resolved {} ({} chars)",
                    r.name,
                    store.get(&r.name).map_or(0, |v| v.len())
                );
                ok += 1;
            }
            Err(e) => {
                error!("failed to resolve {}: {}", r.name, e);
                fail += 1;
            }
        }
    }

    (ok, fail)
}

/// Reads a single secret from 1Password by invoking `op read <uri>`.
///
/// Requires `OP_SERVICE_ACCOUNT_TOKEN` to be set in the environment.
/// The trailing newline from `op read` output is stripped before wrapping
/// the value in a [`SecretString`].
///
/// # Errors
///
/// Returns a descriptive error string if the `op` binary cannot be executed,
/// exits with a non-zero status, or produces non-UTF-8 output.
pub async fn op_read(uri: &str) -> Result<SecretString, String> {
    debug!("op read {uri}");
    let output = tokio::process::Command::new("op")
        .args(["read", uri])
        .output()
        .await
        .map_err(|e| format!("failed to execute op: {e}"))?;
    trace!("op read {uri} -> status={}", output.status);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "op read failed ({}): {}",
            output.status,
            stderr.trim()
        ));
    }

    let value =
        String::from_utf8(output.stdout).map_err(|e| format!("invalid UTF-8 in op output: {e}"))?;

    Ok(SecretString::from(value.trim_end_matches('\n').to_string()))
}

/// Writes a secret value to 1Password by invoking `op item edit`.
///
/// The `uri` must be in `op://vault/item/field` format. This function parses
/// the URI into its components and runs:
///
/// ```text
/// op item edit <item> <field>=<value> --vault <vault>
/// ```
///
/// # Safety boundary
///
/// This function can **update** existing 1Password fields but cannot create
/// or delete items. The security boundary matches the `op` CLI itself.
///
/// # Errors
///
/// Returns a descriptive error string if the URI is malformed, the `op` binary
/// cannot be executed, or `op item edit` exits with a non-zero status.
pub async fn op_write(uri: &str, value: &str) -> Result<(), String> {
    debug!("op item edit for {uri} ({} chars)", value.len());
    let parts: Vec<&str> = uri
        .strip_prefix("op://")
        .ok_or_else(|| format!("invalid op:// URI: {uri}"))?
        .splitn(3, '/')
        .collect();

    if parts.len() < 3 {
        return Err(format!("URI must be op://vault/item/field, got: {uri}"));
    }

    let (vault, item, field) = (parts[0], parts[1], parts[2]);
    let assignment = format!("{field}={value}");

    let output = tokio::process::Command::new("op")
        .args(["item", "edit", item, &assignment, "--vault", vault])
        .output()
        .await
        .map_err(|e| format!("failed to execute op: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "op item edit failed ({}): {}",
            output.status,
            stderr.trim()
        ));
    }

    Ok(())
}
