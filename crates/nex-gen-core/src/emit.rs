//! Emit-tier types: the emitted files.
//!
//! These are computed per language by an [`Emitter`](crate::Emitter).
//! [`EmittedFile`] is one rendered file, including its import block (each
//! emitter renders its own imports via the per-language
//! [`render`](crate::render) helpers).

use std::path::PathBuf;

/// One rendered file produced by an emitter.
///
/// The emitter owns file layout (which files, what's in each) and renders each
/// file's `body` in full, including its import block. [`assemble`](crate::assemble)
/// collects the files by path and chooses the output layout.
#[derive(Clone, Debug)]
pub struct EmittedFile {
    /// Output-relative path for this file.
    pub path: PathBuf,
    /// The fully rendered file contents, including any import block.
    pub body: String,
}
