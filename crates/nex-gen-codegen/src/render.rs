//! Base per-language rendering utilities, reused by every emitter.
//!
//! Two functions, symmetric with each other:
//! [`render_service`] renders a service binding (structural logic + per-
//! language formatting), naming I/O types via a [`NameResolver`] the emitter
//! supplies; [`render_imports`] renders a file's import block. Only **type**
//! rendering is per-schema_type and stays in the front-end crate — service and
//! import rendering are written once per language here.

use std::collections::{BTreeMap, BTreeSet};

use crate::emit::{Import, ImportBinding, Module};
use crate::ir::{ServiceDef, SymbolId};
use crate::language::Language;

/// How the base names and locates a referenced symbol, supplied by the emitter.
///
/// The base never inspects the schema_type's private type data, so when it
/// renders a service it asks the emitter (via this resolver) how to name and
/// import each operation's I/O type by [`SymbolId`].
pub trait NameResolver {
    /// How a referrer names the symbol in source (its emit-tier `type_ref`),
    /// e.g. the local or qualified type name.
    fn type_ref(&self, id: SymbolId) -> String;

    /// The module the symbol is placed in (its emit-tier `module`). Used to
    /// decide same-module vs. cross-module.
    fn module_of(&self, id: SymbolId) -> Module;

    /// How to import the symbol cross-module (its emit-tier `import_binding`).
    fn import_binding(&self, id: SymbolId) -> Import;
}

/// Render a single service binding for `lang`.
///
/// Structural logic (operations, wire names, docs) is language-agnostic;
/// per-language formatting branches on `lang`. I/O type names come from
/// `names`. Phase 2 pulls the real bodies out of `python::generate` /
/// `typescript::generate`.
pub fn render_service(lang: Language, _svc: &ServiceDef, _names: &dyn NameResolver) -> String {
    match lang {
        // Phase 2 extracts the real service/operation rendering out of
        // `{lang}::generate` into this utility (structural logic + per-language
        // formatting, naming I/O types via `names`). Out of scope for the
        // import/assembly step.
        Language::Python | Language::TypeScript => todo!(
            "Phase 2: extract Nexus service-binding rendering for {lang} \
             out of `{lang}::generate` into this base utility"
        ),
        _ => todo!("service rendering for {lang} is not implemented yet"),
    }
}

/// Render the import block for a file, given its resolved [`Import`]s.
///
/// Symmetric with [`render_service`]: structural over `imports`, formatted per
/// `lang`. This is a base utility, **not** an emitter method — the emitter
/// produces structured imports on its [`EmittedFile`](crate::EmittedFile)s and
/// the base renders + stitches the block in [`assemble`](crate::assemble).
///
/// Output is canonical (sorted, grouped); the per-language formatter
/// (`ruff` / `prettier`) reflows it afterwards. [`ImportBinding::Named`]
/// imports to the same module are merged into one statement.
pub fn render_imports(lang: Language, imports: &[Import]) -> String {
    match lang {
        Language::Python => render_python_imports(imports),
        Language::TypeScript => render_typescript_imports(imports),
        _ => todo!("import rendering for {lang} is not implemented yet"),
    }
}

/// Render a Python import block.
///
/// - [`ImportBinding::Module`] / [`ImportBinding::Namespace`] => `import <mod>`
///   (Python imports the whole module path; the alias/name is unused — uses are
///   already qualified through the module path).
/// - [`ImportBinding::Named`] => `from <mod> import (X, Y, ...)`, names merged
///   per module and sorted. (Python has no `import type`, so `type_only` does
///   not change the rendering.)
fn render_python_imports(imports: &[Import]) -> String {
    let mut module_imports: BTreeSet<String> = BTreeSet::new();
    let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for import in imports {
        match import.binding {
            ImportBinding::Module | ImportBinding::Namespace => {
                module_imports.insert(import.module.as_str().to_string());
            }
            ImportBinding::Named => {
                let name = import
                    .name
                    .clone()
                    .unwrap_or_else(|| import.module.as_str().to_string());
                named
                    .entry(import.module.as_str().to_string())
                    .or_default()
                    .insert(name);
            }
        }
    }

    let mut lines: Vec<String> = Vec::new();
    for module in &module_imports {
        lines.push(format!("import {module}"));
    }
    for (module, names) in &named {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        lines.push(render_python_named_import(module, &names));
    }
    lines.join("\n")
}

/// Render one Python `from <module> import (...)` statement.
fn render_python_named_import(module: &str, names: &[&str]) -> String {
    if names.len() == 1 {
        return format!("from {module} import {}", names[0]);
    }
    let body = names
        .iter()
        .map(|name| format!("    {name},"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("from {module} import (\n{body}\n)")
}

/// Render a TypeScript import block.
///
/// - [`ImportBinding::Module`] / [`ImportBinding::Namespace`] =>
///   `import [type] * as <alias> from "<mod>"` (alias from [`Import::name`],
///   defaulting to the module string when absent).
/// - [`ImportBinding::Named`] => `import [type] { X, Y } from "<mod>"`, names
///   merged per module and sorted. The proto namespace-head import is a `Named`
///   import whose single name is the namespace head (e.g. `temporal`).
///
/// `type_only` selects `import type`; type-only and value imports for the same
/// module render as separate statements (they cannot merge).
fn render_typescript_imports(imports: &[Import]) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Module / Namespace: one statement each, kept in input order's sorted
    // form via a set keyed by the rendered line.
    let mut star_lines: BTreeSet<String> = BTreeSet::new();
    // Named, merged per (module, type_only).
    let mut named: BTreeMap<(String, bool), BTreeSet<String>> = BTreeMap::new();

    for import in imports {
        let module = import.module.as_str();
        match import.binding {
            ImportBinding::Module | ImportBinding::Namespace => {
                let alias = import.name.as_deref().unwrap_or(module);
                let type_kw = if import.type_only { "type " } else { "" };
                star_lines.insert(format!(
                    "import {type_kw}* as {alias} from \"{module}\";"
                ));
            }
            ImportBinding::Named => {
                let name = import.name.clone().unwrap_or_else(|| module.to_string());
                named
                    .entry((module.to_string(), import.type_only))
                    .or_default()
                    .insert(name);
            }
        }
    }

    lines.extend(star_lines);
    for ((module, type_only), names) in &named {
        let type_kw = if *type_only { "type " } else { "" };
        let joined = names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "import {type_kw}{{ {joined} }} from \"{module}\";"
        ));
    }
    lines.join("\n")
}
