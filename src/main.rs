#![forbid(unsafe_code)]

use clap::{Parser, Subcommand, ValueEnum};
use op_bridge::{client, resolver, socket, store, watcher};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::RwLock;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "op-bridge", about = "1Password secret broker daemon and CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the secret broker daemon
    Daemon {
        /// Path for the Unix socket
        #[arg(long, default_value = "/tmp/op-bridge.sock")]
        socket: PathBuf,

        /// Environment variable prefix for secret references
        #[arg(long, default_value = "OP_")]
        prefix: String,

        /// Suffix for reference env vars
        #[arg(long, default_value = "_REF")]
        suffix: String,

        /// Watch a file for changes and write back to 1Password.
        /// Format: <path>=<op://uri> or <path>=<name>=<op://uri>
        /// Can be specified multiple times.
        #[arg(long = "watch", value_name = "SPEC")]
        watches: Vec<String>,

        /// Log verbosity level
        #[arg(long, default_value = "info")]
        log_level: LogLevel,
    },

    /// Get a secret value from the daemon
    Get {
        /// Secret reference name
        name: String,

        /// Path to the daemon socket
        #[arg(long, default_value = "/tmp/op-bridge.sock")]
        socket: PathBuf,
    },

    /// Set a secret value (updates in-memory + writes back to 1Password)
    Set {
        /// Secret reference name
        name: String,

        /// 1Password URI (op://vault/item/field)
        uri: String,

        /// Secret value
        value: String,

        /// Path to the daemon socket
        #[arg(long, default_value = "/tmp/op-bridge.sock")]
        socket: PathBuf,
    },

    /// List all secret reference names
    List {
        /// Path to the daemon socket
        #[arg(long, default_value = "/tmp/op-bridge.sock")]
        socket: PathBuf,
    },

    /// Remove a secret from memory (does NOT delete from 1Password)
    Delete {
        /// Secret reference name to remove
        name: String,

        /// Path to the daemon socket
        #[arg(long, default_value = "/tmp/op-bridge.sock")]
        socket: PathBuf,
    },

    /// Re-resolve all secrets from 1Password (sends SIGHUP to daemon)
    Refresh {
        /// Path to the daemon PID file
        #[arg(long, default_value = "/tmp/op-bridge.pid")]
        pid_file: PathBuf,
    },

    /// Check if the daemon is running
    Ping {
        /// Path to the daemon socket
        #[arg(long, default_value = "/tmp/op-bridge.sock")]
        socket: PathBuf,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Daemon {
            socket: socket_path,
            prefix,
            suffix,
            watches,
            log_level,
        } => {
            let level = match log_level {
                LogLevel::Error => "error",
                LogLevel::Warn => "warn",
                LogLevel::Info => "info",
                LogLevel::Debug => "debug",
                LogLevel::Trace => "trace",
            };
            // RUST_LOG env var overrides --log-level flag
            let filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new(format!("op_bridge={level}")));
            tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_env_filter(filter)
                .init();

            run_daemon(socket_path, prefix, suffix, watches).await
        }
        Command::Get { name, socket } => match client::get(&socket, &name).await {
            Ok(value) => {
                print!("{value}");
                Ok(())
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        Command::Set {
            name,
            uri,
            value,
            socket,
        } => match client::set(&socket, &name, &uri, &value).await {
            Ok(()) => {
                eprintln!("ok");
                Ok(())
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        Command::List { socket } => match client::list(&socket).await {
            Ok(names) => {
                for name in names {
                    println!("{name}");
                }
                Ok(())
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        Command::Delete { name, socket } => match client::delete(&socket, &name).await {
            Ok(()) => {
                eprintln!("deleted {name} from memory");
                Ok(())
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
        Command::Refresh { pid_file } => {
            let pid_str = std::fs::read_to_string(&pid_file).map_err(|e| {
                anyhow::anyhow!("failed to read PID file {}: {e}", pid_file.display())
            })?;
            let pid: i32 = pid_str
                .trim()
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid PID in {}: {e}", pid_file.display()))?;
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGHUP,
            )
            .map_err(|e| anyhow::anyhow!("failed to send SIGHUP to PID {pid}: {e}"))?;
            eprintln!("sent SIGHUP to daemon (PID {pid})");
            Ok(())
        }
        Command::Ping { socket } => match client::ping(&socket).await {
            Ok(()) => {
                println!("pong");
                Ok(())
            }
            Err(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        },
    }
}

async fn run_daemon(
    socket_path: PathBuf,
    prefix: String,
    suffix: String,
    watch_specs: Vec<String>,
) -> anyhow::Result<()> {
    // Discover secret references from environment
    let refs = resolver::discover_refs(&prefix, &suffix);
    if refs.is_empty() {
        info!(
            "no secret references found (prefix={}, suffix={})",
            prefix, suffix
        );
    } else {
        info!("discovered {} secret reference(s)", refs.len());
    }

    // Initial resolution
    let store = Arc::new(RwLock::new(store::SecretStore::new()));
    {
        let mut s = store.write().await;
        let (ok, fail) = resolver::resolve_all(&refs, &mut s).await;
        info!("resolved {ok} secret(s), {fail} failed");
    }

    // Start file watchers if any --watch specs provided
    let _watcher = if !watch_specs.is_empty() {
        let entries: Vec<watcher::WatchEntry> = watch_specs
            .iter()
            .map(|spec| watcher::parse_watch_spec(spec).map_err(|e| anyhow::anyhow!(e)))
            .collect::<Result<Vec<_>, _>>()?;

        info!("starting {} file watcher(s)", entries.len());
        let w = watcher::start_watchers(entries, Arc::clone(&store))
            .await
            .map_err(|e| anyhow::anyhow!(e))?;
        Some(w)
    } else {
        None
    };

    // Start socket server
    let listener = socket::bind(&socket_path).await?;
    info!("listening on {}", socket_path.display());

    // Write PID file for `op-bridge refresh` command
    let pid_path = socket_path.with_extension("pid");
    std::fs::write(&pid_path, std::process::id().to_string())?;
    info!("PID file: {}", pid_path.display());

    // Signal handlers
    let mut sighup = signal(SignalKind::hangup())?;
    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let store = Arc::clone(&store);
                        tokio::spawn(async move {
                            if let Err(e) = socket::handle_client(stream, &store).await {
                                error!("client error: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        error!("accept error: {e}");
                    }
                }
            }

            _ = sighup.recv() => {
                info!("SIGHUP received, re-resolving secrets...");
                let mut temp = store::SecretStore::new();
                let (ok, fail) = resolver::resolve_all(&refs, &mut temp).await;
                {
                    let mut s = store.write().await;
                    s.replace_with(temp);
                }
                info!("re-resolved {ok} secret(s), {fail} failed");
            }

            _ = sigterm.recv() => {
                info!("SIGTERM received, shutting down...");
                break;
            }
            _ = sigint.recv() => {
                info!("SIGINT received, shutting down...");
                break;
            }
        }
    }

    // Cleanup
    {
        let mut s = store.write().await;
        s.clear();
        info!("all secrets zeroed");
    }

    // Remove socket and PID file
    for path in [&socket_path, &pid_path] {
        if path.exists() {
            std::fs::remove_file(path)?;
            info!("removed: {}", path.display());
        }
    }

    Ok(())
}
