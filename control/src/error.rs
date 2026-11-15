// SPDX-License-Identifier: AGPL-3.0-only
//! The error of the controller and the players. The REST API and the DLNA
//! renderer each turn the kind into their own error answer.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The request has a value that is not valid
    BadRequest,
    NotFound,
    /// The app or component for the request is not installed
    NotInstalled,
    /// The player or its server does not answer
    Offline,
    /// The request needs media that plays
    NothingPlays,
    /// The player refused the command or failed
    Player,
}

impl ErrorKind {
    /// The fixed code for scripts in the answer of the API
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::BadRequest => "bad_request",
            ErrorKind::NotFound => "not_found",
            ErrorKind::NotInstalled => "not_installed",
            ErrorKind::Offline => "offline",
            ErrorKind::NothingPlays => "nothing_plays",
            ErrorKind::Player => "player_error",
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    /// A sentence for a person
    pub message: String,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        Error {
            kind,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::BadRequest, message)
    }

    pub fn not_found(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::NotFound, message)
    }

    pub fn not_installed(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::NotInstalled, message)
    }

    pub fn offline(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Offline, message)
    }

    pub fn nothing_plays() -> Error {
        Error::new(ErrorKind::NothingPlays, "No player plays media.")
    }

    pub fn player(message: impl Into<String>) -> Error {
        Error::new(ErrorKind::Player, message)
    }
}

/// Only the Jellyfin client uses `?` on reqwest errors. The other HTTP
/// clients map their errors to a message of their own.
impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Error {
        Error::player(format!("Jellyfin request failed: {error}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error::player(error.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.message, self.kind.code())
    }
}

impl std::error::Error for Error {}
