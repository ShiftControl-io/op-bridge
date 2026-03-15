//! In-memory secret store with zeroize-on-drop guarantees.
//!
//! All secret values are stored as [`secrecy::SecretString`], which uses
//! [`zeroize::Zeroize`] to scrub memory when values are dropped. The store
//! also tracks the 1Password `op://` URI for each secret, enabling write-back
//! via [`crate::resolver::op_write`].

use secrecy::{ExposeSecret, SecretString};
use std::collections::HashMap;

/// Internal entry pairing a secret value with its optional 1Password URI.
///
/// The URI is [`Some`] when the secret was resolved from a known `op://`
/// reference, and [`None`] when inserted without origin tracking (e.g., tests).
struct SecretEntry {
    value: SecretString,
    uri: Option<String>,
}

/// Thread-safe, in-memory secret store.
///
/// Designed to be wrapped in an [`Arc<RwLock<SecretStore>>`] for concurrent
/// access from the socket server and signal handlers. All mutations that
/// remove or replace entries trigger [`SecretString`]'s zeroize-on-drop,
/// ensuring secret material is scrubbed from memory immediately.
///
/// # Examples
///
/// ```
/// use op_bridge::store::SecretStore;
/// use secrecy::SecretString;
///
/// let mut store = SecretStore::new();
/// store.insert_with_uri(
///     "TOKEN".into(),
///     SecretString::from("s3cret".to_string()),
///     "op://vault/item/field".into(),
/// );
/// assert_eq!(store.get("TOKEN"), Some("s3cret"));
/// assert_eq!(store.get_uri("TOKEN"), Some("op://vault/item/field"));
/// ```
#[derive(Default)]
pub struct SecretStore {
    entries: HashMap<String, SecretEntry>,
}

impl SecretStore {
    /// Creates an empty secret store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a secret without origin URI tracking.
    ///
    /// Prefer [`insert_with_uri`](Self::insert_with_uri) when the `op://` URI
    /// is known, as it enables write-back support.
    pub fn insert(&mut self, name: String, value: SecretString) {
        self.entries.insert(name, SecretEntry { value, uri: None });
    }

    /// Inserts a secret along with its 1Password `op://` URI.
    ///
    /// The URI is stored alongside the value so that [`crate::resolver::op_write`]
    /// can write updated values back to the correct 1Password field.
    pub fn insert_with_uri(&mut self, name: String, value: SecretString, uri: String) {
        self.entries.insert(
            name,
            SecretEntry {
                value,
                uri: Some(uri),
            },
        );
    }

    /// Returns the exposed secret value for the given reference name, or [`None`].
    ///
    /// # Security
    ///
    /// The returned `&str` borrows from the underlying [`SecretString`]. Avoid
    /// copying it into long-lived allocations — prefer passing it directly to
    /// the socket response writer.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(name).map(|e| e.value.expose_secret())
    }

    /// Returns the 1Password `op://` URI for the given reference name, if known.
    pub fn get_uri(&self, name: &str) -> Option<&str> {
        self.entries.get(name).and_then(|e| e.uri.as_deref())
    }

    /// Returns all stored reference names (not values).
    ///
    /// The order is arbitrary (HashMap iteration order).
    pub fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(|k| k.as_str()).collect()
    }

    /// Removes a single secret from memory.
    ///
    /// The dropped [`SecretString`] is zeroized automatically. Returns `true`
    /// if the entry existed. This operation does **not** delete anything from
    /// 1Password — it only affects the in-memory store.
    pub fn remove(&mut self, name: &str) -> bool {
        self.entries.remove(name).is_some()
    }

    /// Removes all secrets from the store, zeroizing each on drop.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Atomically replaces the entire store contents with `other`.
    ///
    /// Used by the SIGHUP refresh path to minimize write-lock hold time:
    /// secrets are resolved into a temporary store, then swapped in with a
    /// single assignment (microseconds vs. seconds of resolution time).
    pub fn replace_with(&mut self, other: SecretStore) {
        self.entries = other.entries;
    }
}
