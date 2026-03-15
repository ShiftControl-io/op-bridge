#![forbid(unsafe_code)]

//! # op-bridge
//!
//! A lightweight 1Password secret broker daemon for Docker containers.
//!
//! `op-bridge` resolves secrets from 1Password once at startup, holds them in
//! memory using [`secrecy::SecretString`] (mlock'd, zeroized on drop), and
//! serves them to local processes via a Unix domain socket.
//!
//! ## Modules
//!
//! - [`client`] — Unix socket client for querying the daemon from CLI subcommands.
//! - [`resolver`] — 1Password CLI integration (`op read` / `op item edit`).
//! - [`socket`] — Unix socket server implementing the line-based protocol.
//! - [`store`] — In-memory secret store with zeroize-on-drop guarantees.
//! - [`watcher`] — File system watcher for credential auto-sync back to 1Password.

pub mod client;
pub mod resolver;
pub mod socket;
pub mod store;
pub mod watcher;
