//! Structured wire errors and their CLI exit-code mapping.

use serde::{Deserialize, Serialize};

use crate::protocol::{LegalActions, Turn};

/// Error classes that may appear on the wire.
///
/// `no_session` (3) and `unreachable` (4) are client-side only and never travel
/// from the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ErrorCode {
    Internal,
    Usage,
    #[serde(rename = "protocol_version")]
    ProtocolVersion,
    Auth,
    #[serde(rename = "not_found")]
    NotFound,
    #[serde(rename = "not_your_turn")]
    NotYourTurn,
    #[serde(rename = "illegal_action")]
    IllegalAction,
    Stale,
    #[serde(rename = "table_finished")]
    TableFinished,
    #[serde(rename = "wait_timeout")]
    WaitTimeout,
}

impl ErrorCode {
    /// Process exit code the CLI must use for this error class.
    pub const fn exit_code(self) -> i32 {
        match self {
            Self::Internal => 1,
            Self::Usage => 2,
            Self::ProtocolVersion => 5,
            Self::Auth => 6,
            Self::NotFound => 7,
            Self::NotYourTurn => 10,
            Self::IllegalAction => 11,
            Self::Stale => 12,
            Self::TableFinished => 13,
            Self::WaitTimeout => 14,
        }
    }
}

/// The `error` object of a failed response.
///
/// Correctable failures (`not_your_turn`, `illegal_action`, `stale`) carry the
/// context needed to retry: the current turn, a fresh `legal_actions` menu, and
/// (for `stale`) the current `seq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<Turn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legal_actions: Option<LegalActions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
}

impl ErrorBody {
    /// Construct an error with no correction context.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            turn: None,
            legal_actions: None,
            seq: None,
        }
    }

    /// Malformed request, arguments, or command.
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Usage, message)
    }

    /// Bad or missing token, or insufficient role.
    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Auth, message)
    }

    /// No such table, seat, or hand.
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// Unexpected server bug.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Internal, message)
    }
}
