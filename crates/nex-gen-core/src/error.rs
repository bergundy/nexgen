//! Crate-local error type.
//!
//! A trimmed subset of the existing crate's `src/error.rs` — only the variants
//! the core layer (output plumbing + assembly + load/emit) needs. Front-ends
//! add their own validation errors in their own crates; those map into
//! [`Error::Load`] (or are surfaced via the front-end's own error type) rather
//! than living here.

use std::io;
use std::path::PathBuf;
use std::process::ExitStatus;

use thiserror::Error;

use crate::language::Language;

pub type Result<T> = std::result::Result<T, Error>;

/// Errors produced by the core codegen layer.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to read `{path}`: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to write `{path}`: {source}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("refusing to overwrite existing path `{path}`")]
    OutputPathExists { path: PathBuf },

    #[error("failed to run formatter `{command}` for `{path}`: {source}")]
    RunFormatter {
        path: PathBuf,
        command: String,
        #[source]
        source: io::Error,
    },

    #[error("formatter `{command}` failed for `{path}` with status {status}")]
    FormatterFailed {
        path: PathBuf,
        command: String,
        status: ExitStatus,
    },

    #[error("language `{language}` is not implemented yet")]
    UnsupportedLanguage { language: Language },

    /// A loader failed while validating inputs / building the IR. The message
    /// is the front-end's own diagnostic; the core does not interpret it.
    #[error("failed to load inputs: {message}")]
    Load { message: String },
}
