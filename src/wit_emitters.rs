//! WIT front-end emitters that render straight from `IR<WitSymbolKind>`.
//!
//! Each emitter builds a borrowing [`WitSymbols`] view over the symbol table
//! and runs the per-language renderers (which render each file's body in full,
//! import block included), then routes the result through the core
//! [`assemble`](nex_gen_core::assemble) pipeline. The emitter derives everything
//! it needs from the IR, satisfying the "emitter works only from the IR"
//! contract.
//!
//! `assemble` collects the files by path and picks the layout, so the output is
//! byte-identical to the legacy `generate_files` path (guarded by the
//! equivalence tests below and the checked-in example suites).

use nex_gen_core::{Emitter, EmittedFile, Error, Language, Result, IR};

use crate::wit_symbols::{WitSymbolKind, WitSymbols};
use crate::generator::GeneratedFiles;

/// Wrap a legacy [`GeneratedFiles`] map into core [`EmittedFile`]s.
///
/// Each file's body already contains its import block, so assembly is a pure
/// collect + layout pass that reproduces the map.
fn into_emitted_files(generated: GeneratedFiles) -> Vec<EmittedFile> {
    generated
        .files
        .into_iter()
        .map(|(path, body)| EmittedFile { path, body })
        .collect()
}

/// The Python WIT emitter. Support fragments travel through the IR as
/// [`Fragment`](crate::wit_symbols::WitSymbolKind::Fragment) symbols, so the
/// emitter is stateless.
pub(crate) struct PythonEmitter;

impl PythonEmitter {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Emitter<WitSymbolKind> for PythonEmitter {
    fn language(&self) -> Language {
        Language::Python
    }

    fn emit(&self, ir: &IR<WitSymbolKind>) -> Result<Vec<EmittedFile>> {
        let symbols = WitSymbols::new(&ir.symbols);
        let generated = crate::python::generate(&symbols).map_err(|error| Error::Load {
            message: error.to_string(),
        })?;
        Ok(into_emitted_files(generated))
    }
}

/// The TypeScript WIT emitter.
pub(crate) struct TypeScriptEmitter;

impl TypeScriptEmitter {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Emitter<WitSymbolKind> for TypeScriptEmitter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn emit(&self, ir: &IR<WitSymbolKind>) -> Result<Vec<EmittedFile>> {
        let symbols = WitSymbols::new(&ir.symbols);
        let generated = crate::typescript::generate(&symbols).map_err(|error| Error::Load {
            message: error.to_string(),
        })?;
        Ok(into_emitted_files(generated))
    }
}

/// The .NET WIT emitter.
pub(crate) struct DotnetEmitter;

impl DotnetEmitter {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl Emitter<WitSymbolKind> for DotnetEmitter {
    fn language(&self) -> Language {
        Language::Dotnet
    }

    fn emit(&self, ir: &IR<WitSymbolKind>) -> Result<Vec<EmittedFile>> {
        let symbols = WitSymbols::new(&ir.symbols);
        let generated = crate::dotnet::generate(&symbols).map_err(|error| Error::Load {
            message: error.to_string(),
        })?;
        Ok(into_emitted_files(generated))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nex_gen_core::{assemble, IR};
    use prost_types::FileDescriptorSet;

    use super::{DotnetEmitter, PythonEmitter, TypeScriptEmitter};
    use crate::wit_symbols::{build_wit_symbols, WitSymbolKind, WitSymbols};
    use crate::descriptors::DescriptorIndex;
    use crate::spec::ApiSpec;
    use crate::Language;

    const INLINE_WIT: &str = r#"
package temporal:users@1.0.0;

world system {
  export user-service;
}

/// @nexus.endpoint "__user_service"
interface user-service {
  enum status {
    active,
    disabled,
  }

  record update-email-request {
    users-id: string,
    email: string,
    status: status,
  }

  record update-email-response {
    ok: bool,
  }

  update-email: func(request: update-email-request) -> update-email-response;
}
"#;

    /// Build the IR the WIT loader would produce for the inline schema.
    fn build_ir(language: Language) -> IR<WitSymbolKind> {
        let spec =
            ApiSpec::parse_for_language(language, INLINE_WIT, PathBuf::from("inline.wit")).unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        IR::new(build_wit_symbols(&spec, &descriptors, &[]).unwrap())
    }

    #[test]
    fn python_emitter_matches_direct_generate() {
        let ir = build_ir(Language::Python);
        let expected = crate::python::generate(&WitSymbols::new(&ir.symbols)).unwrap();
        let emitter = PythonEmitter::new();
        let assembled = assemble(&ir, &emitter).unwrap();
        assert_eq!(assembled.files, expected.files);
    }

    #[test]
    fn typescript_emitter_matches_direct_generate() {
        let ir = build_ir(Language::TypeScript);
        let expected = crate::typescript::generate(&WitSymbols::new(&ir.symbols)).unwrap();
        let emitter = TypeScriptEmitter::new();
        let assembled = assemble(&ir, &emitter).unwrap();
        assert_eq!(assembled.files, expected.files);
    }

    #[test]
    fn dotnet_emitter_matches_direct_generate() {
        let ir = build_ir(Language::Dotnet);
        let expected = crate::dotnet::generate(&WitSymbols::new(&ir.symbols)).unwrap();
        let emitter = DotnetEmitter::new();
        let assembled = assemble(&ir, &emitter).unwrap();
        assert_eq!(assembled.files, expected.files);
    }
}
