//! WIT front-end emitters that render straight from `IR<WitSymbolKind>`.
//!
//! Each emitter reconstructs the WIT-internal [`ApiPlan`](crate::api_plan::ApiPlan)
//! from the symbol table (via [`symbols_to_plan`]) and runs the existing
//! per-language renderers, then routes the result through the base
//! [`assemble`](nex_gen_codegen::assemble) pipeline. The base never sees an
//! `ApiPlan`: the emitter derives everything it needs from the IR, satisfying
//! the "emitter works only from the IR" contract.
//!
//! These emitters currently keep their import blocks inlined in each file's
//! body and declare no `refs`, so `assemble` runs as a stitch/layout pass and
//! the output is byte-identical to the legacy `generate_files` path (guarded by
//! the equivalence tests below and the checked-in example suites). Moving
//! import resolution itself into the base (populating `refs` + `render_imports`)
//! is a follow-up that requires extending the base import model.

use nex_gen_codegen::{
    Emitter, EmittedFile, Error, Import, Language, Module, NameResolver, Result, SchemaType,
    SymbolId, IR,
};

use crate::generator::GeneratedFiles;
use crate::spec::SupportFragmentSpec;
use crate::wit_loader::{symbols_to_plan, WitSymbolKind};

/// A resolver that is never consulted.
///
/// These emitters render each file's imports inline and declare no `refs`, so
/// [`assemble`](nex_gen_codegen::assemble) never resolves a symbol through the
/// resolver. It exists only to satisfy the [`Emitter::resolver`] contract; if a
/// method is ever called it is a bug (a `ref` leaked without matching wiring).
struct UnusedResolver;

impl NameResolver for UnusedResolver {
    fn type_ref(&self, id: SymbolId) -> String {
        unreachable!("resolver consulted for {id:?} but these emitters declare no refs")
    }

    fn module_of(&self, id: SymbolId) -> Module {
        unreachable!("resolver consulted for {id:?} but these emitters declare no refs")
    }

    fn import_binding(&self, id: SymbolId) -> Import {
        unreachable!("resolver consulted for {id:?} but these emitters declare no refs")
    }
}

/// Wrap a legacy [`GeneratedFiles`] map into base [`EmittedFile`]s.
///
/// Each file's body already contains its import block, so it carries no `refs`
/// and no `runtime_imports`; `module` is set to the path so it is stable but is
/// never used (there are no cross-module refs to resolve). `assemble` renders an
/// empty import block for each and stitches it (a no-op), reproducing the map.
fn into_emitted_files(generated: GeneratedFiles) -> Vec<EmittedFile> {
    generated
        .files
        .into_iter()
        .map(|(path, body)| EmittedFile {
            module: Module::new(path.to_string_lossy().into_owned()),
            path,
            refs: Vec::new(),
            runtime_imports: Vec::new(),
            body,
        })
        .collect()
}

/// The Python WIT emitter. Holds the support fragments to render (the frontend
/// supplies them at construction, since they come from the parsed spec).
pub(crate) struct PythonEmitter {
    support_fragments: Vec<SupportFragmentSpec>,
    resolver: UnusedResolver,
}

impl PythonEmitter {
    pub(crate) fn new(support_fragments: Vec<SupportFragmentSpec>) -> Self {
        Self {
            support_fragments,
            resolver: UnusedResolver,
        }
    }
}

impl Emitter<WitSymbolKind> for PythonEmitter {
    fn language(&self) -> Language {
        Language::Python
    }

    fn schema_type(&self) -> SchemaType {
        SchemaType::Wit
    }

    fn emit(&self, ir: &IR<WitSymbolKind>) -> Result<Vec<EmittedFile>> {
        let plan = symbols_to_plan(ir);
        let generated = crate::python::generate(&plan, &self.support_fragments)
            .map_err(|error| Error::Load {
                message: error.to_string(),
            })?;
        Ok(into_emitted_files(generated))
    }

    fn resolver(&self) -> &dyn NameResolver {
        &self.resolver
    }
}

/// The TypeScript WIT emitter.
pub(crate) struct TypeScriptEmitter {
    support_fragments: Vec<SupportFragmentSpec>,
    resolver: UnusedResolver,
}

impl TypeScriptEmitter {
    pub(crate) fn new(support_fragments: Vec<SupportFragmentSpec>) -> Self {
        Self {
            support_fragments,
            resolver: UnusedResolver,
        }
    }
}

impl Emitter<WitSymbolKind> for TypeScriptEmitter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn schema_type(&self) -> SchemaType {
        SchemaType::Wit
    }

    fn emit(&self, ir: &IR<WitSymbolKind>) -> Result<Vec<EmittedFile>> {
        let plan = symbols_to_plan(ir);
        let generated = crate::typescript::generate(&plan, &self.support_fragments)
            .map_err(|error| Error::Load {
                message: error.to_string(),
            })?;
        Ok(into_emitted_files(generated))
    }

    fn resolver(&self) -> &dyn NameResolver {
        &self.resolver
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nex_gen_codegen::{assemble, IR};
    use prost_types::FileDescriptorSet;

    use super::{PythonEmitter, TypeScriptEmitter};
    use crate::api_plan::build_api_plan;
    use crate::descriptors::DescriptorIndex;
    use crate::spec::ApiSpec;
    use crate::wit_loader::plan_to_symbols;
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

    /// Build the IR the WIT loader would produce for the inline schema, plus the
    /// plan the legacy path would build, from the *same* spec + descriptors.
    fn build_plan_and_ir(
        language: Language,
    ) -> (crate::api_plan::ApiPlan, IR<super::WitSymbolKind>) {
        let spec =
            ApiSpec::parse_for_language(language, INLINE_WIT, PathBuf::from("inline.wit")).unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        let plan = build_api_plan(&spec, &descriptors).unwrap();
        let ir = IR::new(plan_to_symbols(plan.clone()));
        (plan, ir)
    }

    #[test]
    fn python_emitter_matches_legacy_generate() {
        let (plan, ir) = build_plan_and_ir(Language::Python);
        let expected = crate::python::generate(&plan, &[]).unwrap();
        let emitter = PythonEmitter::new(Vec::new());
        let assembled = assemble(&ir, &emitter).unwrap();
        assert_eq!(assembled.files, expected.files);
    }

    #[test]
    fn typescript_emitter_matches_legacy_generate() {
        let (plan, ir) = build_plan_and_ir(Language::TypeScript);
        let expected = crate::typescript::generate(&plan, &[]).unwrap();
        let emitter = TypeScriptEmitter::new(Vec::new());
        let assembled = assemble(&ir, &emitter).unwrap();
        assert_eq!(assembled.files, expected.files);
    }
}
