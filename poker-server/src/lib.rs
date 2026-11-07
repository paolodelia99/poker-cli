//! `poker-server`: the single-writer table authority for `poker-cli`.
//!
//! This crate owns tables, tokens, the turn clock, the canonical event log, and
//! the Unix-socket JSONL protocol. The engine remains the only poker authority;
//! the server projects engine state into the shared wire model below.

pub mod config;
pub mod engine;
pub mod errors;
pub mod events;
pub mod persistence;
pub mod protocol;
pub mod views;

pub use config::{default_socket_path, Config};
pub use errors::{ErrorBody, ErrorCode};
pub use protocol::*;
