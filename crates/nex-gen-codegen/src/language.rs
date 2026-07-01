//! Target output languages.
//!
//! Mirror of the existing crate's `src/language.rs`. Duplicated here for now;
//! Phase 3 may unify the two. The base layer is language-aware (placement,
//! import rendering, and service rendering are per-language) but
//! frontend-agnostic.

use std::fmt;

/// A target language an emitter can render for.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Language {
    Dotnet,
    Go,
    Java,
    Python,
    Ruby,
    TypeScript,
}

impl Language {
    /// Stable, lowercase identifier used in paths, CLI flags, and emitter keys.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dotnet => "dotnet",
            Self::Go => "go",
            Self::Java => "java",
            Self::Python => "python",
            Self::Ruby => "ruby",
            Self::TypeScript => "typescript",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
