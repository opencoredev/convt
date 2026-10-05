use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown format: {0}")]
    UnknownFormat(String),
    #[error("can't tell the format of {0}")]
    UndetectedFormat(PathBuf),
    #[error("no way to convert {from} to {to}")]
    NoRoute { from: String, to: String },
    #[error("invalid option: {0}")]
    InvalidOption(String),
    #[error("{engine} is not available: {reason}")]
    EngineUnavailable {
        engine: &'static str,
        reason: String,
    },
    #[error("{engine} failed: {message}")]
    EngineFailed {
        engine: &'static str,
        message: String,
    },
    #[error("{0} already exists")]
    OutputExists(PathBuf),
    #[error("cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    /// A stable, machine-readable name for the error, used in JSON output.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::UnknownFormat(_) | Self::UndetectedFormat(_) => "unsupported_input",
            Self::NoRoute { .. } => "no_route",
            Self::InvalidOption(_) => "invalid_option",
            Self::EngineUnavailable { .. } => "engine_missing",
            Self::EngineFailed { .. } => "engine_failed",
            Self::OutputExists(_) => "output_exists",
            Self::Cancelled => "cancelled",
            Self::Io(_) => "io",
        }
    }
}

/// The last few lines of a tool's stderr, for error messages.
pub fn stderr_tail(stderr: &[u8]) -> String {
    const LINES: usize = 12;
    const BYTES: usize = 2000;
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text.trim().lines().collect();
    let mut tail = lines[lines.len().saturating_sub(LINES)..].join("\n");
    if tail.len() > BYTES {
        let mut cut = tail.len() - BYTES;
        while !tail.is_char_boundary(cut) {
            cut += 1;
        }
        tail = format!("…{}", &tail[cut..]);
    }
    if tail.is_empty() {
        "no error output".into()
    } else {
        tail
    }
}
