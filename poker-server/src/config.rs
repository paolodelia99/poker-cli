//! Server configuration and path defaults.

use std::path::PathBuf;

/// Runtime configuration for a `serve` process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Unix socket the server binds.
    pub socket: PathBuf,
    /// Optional diagnostic log file; diagnostics always go to stderr too.
    pub log: Option<PathBuf>,
    /// Directory that receives public hand documents.
    pub history_dir: PathBuf,
    /// Capacity of the per-table broadcast channel.
    pub broadcast_capacity: usize,
}

impl Config {
    /// Build a default configuration bound to `socket`.
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: socket.into(),
            log: None,
            history_dir: PathBuf::from(".poker/history"),
            broadcast_capacity: 256,
        }
    }
}

/// Default socket path: `$XDG_RUNTIME_DIR/poker/main.sock`, else the system
/// temporary directory's `poker/main.sock`.
pub fn default_socket_path() -> PathBuf {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        if !runtime_dir.is_empty() {
            return PathBuf::from(runtime_dir).join("poker").join("main.sock");
        }
    }
    std::env::temp_dir().join("poker").join("main.sock")
}
