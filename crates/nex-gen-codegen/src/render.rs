//! Base per-language rendering utilities, reused by every emitter.
//!
//! Two functions, symmetric with each other:
//! [`render_service`] renders a service binding (structural logic + per-
//! language formatting), naming I/O types via a [`NameResolver`] the emitter
//! supplies; [`render_imports`] renders a file's import block. Only **type**
//! rendering is per-schema_type and stays in the front-end crate — service and
//! import rendering are written once per language here.

use std::collections::{BTreeMap, BTreeSet};

use heck::{ToLowerCamelCase, ToSnakeCase};

use crate::emit::{Import, ImportBinding, Module};
use crate::ir::{Service, SymbolId};
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
/// `names`. Type rendering, proto conversion, and resources stay in the
/// front-end crate; only the service/operation binding is rendered here.
pub fn render_service(lang: Language, svc: &Service, names: &dyn NameResolver) -> String {
    match lang {
        Language::TypeScript => render_typescript_service(svc, names),
        Language::Python => render_python_service(svc, names),
        _ => todo!("service rendering for {lang} is not implemented yet"),
    }
}

/// TypeScript output line length used for doc-comment wrapping. Mirrors the
/// front-end crate's `TYPESCRIPT_FORMAT_LINE_LENGTH`.
const TYPESCRIPT_FORMAT_LINE_LENGTH: usize = 88;

/// The experimental-warning text emitted as the `@experimental` doc tag.
const EXPERIMENTAL_WARNING: &str = "This API is experimental and subject to change.";

/// Render the TypeScript Nexus service binding
/// (`export const X = nexus.service('wire', { op: nexus.operation<In, Out>({ name: ... }) });`).
///
/// Operation I/O type names come from `names` (`type_ref`), never from
/// proto/WIT knowledge.
fn render_typescript_service(svc: &Service, names: &dyn NameResolver) -> String {
    let mut output = String::new();
    let service_doc_tags = experimental_doc_tag(svc.experimental);
    render_typescript_doc_comment(&mut output, "", None, &service_doc_tags);
    output.push_str("export const ");
    output.push_str(svc.name.as_str());
    output.push_str(" = nexus.service('");
    output.push_str(&svc.wire_name);
    output.push_str("', {\n");
    for operation in &svc.operations {
        let operation_doc_tags = experimental_doc_tag(operation.experimental);
        render_typescript_doc_comment(&mut output, "  ", None, &operation_doc_tags);
        output.push_str("  ");
        output.push_str(&typescript_operation_attr_name(operation.name.as_str()));
        output.push_str(": nexus.operation<\n");
        output.push_str("    ");
        output.push_str(&typescript_operation_io_ref(operation.input, names));
        output.push_str(",\n");
        output.push_str("    ");
        output.push_str(&typescript_operation_io_ref(operation.output, names));
        output.push('\n');
        output.push_str("  >({ name: ");
        output.push_str(&typescript_string_literal(&operation.wire_name));
        output.push_str(" }),\n");
    }
    output.push_str("});\n\n");
    output
}

/// Resolve an operation I/O symbol to its TypeScript type name, defaulting to
/// `void` when the operation has no input/output (matching the front-end's
/// `PlannedOperationOutput::None` -> `"void"`).
fn typescript_operation_io_ref(id: Option<SymbolId>, names: &dyn NameResolver) -> String {
    match id {
        Some(id) => names.type_ref(id),
        None => "void".to_string(),
    }
}

/// The attribute name an operation is bound under in the service object: the
/// canonical name lower-camel-cased and escaped against TS keywords. Mirrors
/// the front-end's `typescript_ident(name.to_lower_camel_case())`.
fn typescript_operation_attr_name(name: &str) -> String {
    typescript_ident(&name.to_lower_camel_case())
}

/// Escape a name that collides with a TypeScript keyword by suffixing `_`.
/// Mirrors the front-end's `typescript_ident`.
fn typescript_ident(name: &str) -> String {
    if is_typescript_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

fn is_typescript_keyword(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "as"
            | "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
    )
}

/// A TypeScript string literal, matching the front-end's `{value:?}` form.
fn typescript_string_literal(value: &str) -> String {
    format!("{value:?}")
}

fn experimental_doc_tag(experimental: bool) -> Vec<(String, String)> {
    if experimental {
        vec![(
            "@experimental".to_string(),
            EXPERIMENTAL_WARNING.to_string(),
        )]
    } else {
        Vec::new()
    }
}

/// Render a TypeScript JSDoc comment from an optional summary and tags.
/// Mirrors the front-end's `render_typescript_doc_comment`.
fn render_typescript_doc_comment(
    output: &mut String,
    indent: &str,
    summary: Option<&str>,
    tags: &[(String, String)],
) {
    let has_summary = summary.is_some_and(|summary| !summary.trim().is_empty());
    let has_tags = tags.iter().any(|(_, doc)| !doc.trim().is_empty());
    if !has_summary && !has_tags {
        return;
    }

    output.push_str(indent);
    output.push_str("/**\n");
    if let Some(summary) = summary.map(str::trim).filter(|summary| !summary.is_empty()) {
        for line in summary.lines() {
            push_wrapped_typescript_doc_line(output, indent, "", "", line.trim());
        }
    }
    if has_summary && has_tags {
        output.push_str(indent);
        output.push_str(" *\n");
    }
    for (tag, doc) in tags {
        let doc = doc.trim();
        if doc.is_empty() {
            continue;
        }
        push_wrapped_typescript_doc_line(output, indent, &format!("{tag} "), "  ", doc);
    }
    output.push_str(indent);
    output.push_str(" */\n");
}

/// Word-wrap one JSDoc line. Mirrors the front-end's
/// `push_wrapped_typescript_doc_line`.
fn push_wrapped_typescript_doc_line(
    output: &mut String,
    indent: &str,
    first_prefix: &str,
    continuation_prefix: &str,
    text: &str,
) {
    let max_width = TYPESCRIPT_FORMAT_LINE_LENGTH.saturating_sub(indent.chars().count() + 3);
    let text = text.replace("*/", "* /");
    if text.trim().is_empty() {
        output.push_str(indent);
        output.push_str(" *\n");
        return;
    }

    let mut prefix = first_prefix;
    let mut current = String::new();
    for word in text.split_whitespace() {
        let prefix_width = prefix.chars().count();
        let current_width = current.chars().count();
        let word_width = word.chars().count();
        let separator_width = usize::from(!current.is_empty());
        if current_width > 0
            && prefix_width + current_width + separator_width + word_width > max_width
        {
            output.push_str(indent);
            output.push_str(" * ");
            output.push_str(prefix);
            output.push_str(&current);
            output.push('\n');
            prefix = continuation_prefix;
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    output.push_str(indent);
    output.push_str(" * ");
    output.push_str(prefix);
    output.push_str(&current);
    output.push('\n');
}

/// Python output line length used for docstring wrapping. Mirrors the
/// front-end crate's `PYTHON_FORMAT_LINE_LENGTH`.
const PYTHON_FORMAT_LINE_LENGTH: usize = 88;

/// Render the Python Nexus service binding (`@service` decorated `class` whose
/// operations are `Operation[In, Out]` class attributes).
///
/// Operation I/O type names come from `names` (`type_ref`); the front-end
/// adapter passes already-resolved (and Python-placement-stripped) refs.
fn render_python_service(svc: &Service, names: &dyn NameResolver) -> String {
    let mut output = String::new();
    if svc.wire_name == svc.name.as_str() {
        output.push_str("@service\n");
    } else {
        output.push_str("@service(name=");
        output.push_str(&python_string_literal(&svc.wire_name));
        output.push_str(")\n");
    }
    output.push_str("class ");
    output.push_str(svc.name.as_str());
    output.push_str(":\n");
    render_python_docstring(&mut output, "    ", None, &[], None, svc.experimental);

    if svc.operations.is_empty() {
        if !svc.experimental {
            output.push_str("    pass\n");
        }
        return output;
    }

    for (operation_index, operation) in svc.operations.iter().enumerate() {
        if operation.experimental {
            output.push_str("    # ");
            output.push_str(".. warning:: ");
            output.push_str(EXPERIMENTAL_WARNING);
            output.push('\n');
        }
        output.push_str("    ");
        output.push_str(&python_operation_attr_name(operation.name.as_str()));
        output.push_str(": Operation[\n");
        output.push_str("        ");
        output.push_str(&python_operation_io_ref(operation.input, names));
        output.push_str(",\n");
        output.push_str("        ");
        output.push_str(&python_operation_io_ref(operation.output, names));
        output.push_str(",\n");
        output.push_str("    ] = Operation(name=");
        output.push_str(&python_string_literal(&operation.wire_name));
        output.push_str(")\n");

        if operation_index + 1 != svc.operations.len() {
            output.push('\n');
        }
    }
    output
}

/// Resolve an operation I/O symbol to its Python type name. The WIT front-end
/// always supplies both refs as concrete types (Python's `Operation[...]` lists
/// both, with no `void`/`None` collapsing), so `None` never occurs for current
/// inputs; we fall back to `None` (Python's no-value type name) defensively.
fn python_operation_io_ref(id: Option<SymbolId>, names: &dyn NameResolver) -> String {
    match id {
        Some(id) => names.type_ref(id),
        None => "None".to_string(),
    }
}

/// The attribute name an operation is bound under on the service class: the
/// canonical name snake-cased and escaped against Python keywords. Mirrors the
/// front-end's `python_ident(name.to_snake_case())`.
fn python_operation_attr_name(name: &str) -> String {
    python_ident(&name.to_snake_case())
}

/// Escape a name that collides with a Python keyword by suffixing `_`. Mirrors
/// the front-end's `python_ident`.
fn python_ident(name: &str) -> String {
    if is_python_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

fn is_python_keyword(name: &str) -> bool {
    matches!(
        name,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
            | "match"
            | "case"
    )
}

/// A Python string literal, matching the front-end's `{value:?}` form.
fn python_string_literal(value: &str) -> String {
    format!("{value:?}")
}

/// Render a Python docstring from an optional summary, args, returns, and the
/// experimental flag. Mirrors the front-end's `render_python_docstring`. The
/// service-binding path only ever passes `summary = None`, no args, no returns,
/// and an `experimental` flag, but the full logic is ported byte-for-byte.
fn render_python_docstring(
    output: &mut String,
    indent: &str,
    summary: Option<&str>,
    args: &[(String, String)],
    returns: Option<&str>,
    experimental: bool,
) {
    let mut lines = Vec::<String>::new();
    let docstring_width = PYTHON_FORMAT_LINE_LENGTH.saturating_sub(indent.chars().count());
    let has_summary = summary.is_some_and(|summary| !summary.trim().is_empty());
    if let Some(summary) = summary.map(str::trim).filter(|summary| !summary.is_empty()) {
        for line in summary.lines() {
            push_wrapped_python_docstring_line(&mut lines, "", "", line.trim(), docstring_width);
        }
    }
    if experimental {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(".. warning::".to_string());
        push_wrapped_python_docstring_line(
            &mut lines,
            "    ",
            "    ",
            EXPERIMENTAL_WARNING,
            docstring_width,
        );
    }
    if !args.is_empty() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Args:".to_string());
        for (name, doc) in args {
            let mut doc_lines = doc.trim().lines();
            let first_prefix = format!("    {name}: ");
            let continuation_prefix = "        ";
            let first = doc_lines.next().unwrap_or_default().trim();
            push_wrapped_python_docstring_line(
                &mut lines,
                &first_prefix,
                continuation_prefix,
                first,
                docstring_width,
            );
            for line in doc_lines {
                push_wrapped_python_docstring_line(
                    &mut lines,
                    continuation_prefix,
                    continuation_prefix,
                    line.trim(),
                    docstring_width,
                );
            }
        }
    }
    if let Some(returns) = returns.map(str::trim).filter(|returns| !returns.is_empty()) {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push("Returns:".to_string());
        let mut return_lines = returns.lines();
        if let Some(first) = return_lines.next() {
            push_wrapped_python_docstring_line(
                &mut lines,
                "    ",
                "    ",
                first.trim(),
                docstring_width,
            );
        }
        for line in return_lines {
            push_wrapped_python_docstring_line(
                &mut lines,
                "    ",
                "    ",
                line.trim(),
                docstring_width,
            );
        }
    }
    if lines.is_empty() {
        return;
    }

    output.push_str(indent);
    output.push_str("\"\"\"");
    if !has_summary {
        output.push('\n');
        for line in &lines {
            if !line.is_empty() {
                output.push_str(indent);
                output.push_str(&python_docstring_literal_text(line));
            }
            output.push('\n');
        }
        output.push_str(indent);
        output.push_str("\"\"\"\n");
        return;
    }
    if lines.len() == 1 {
        output.push_str(&python_docstring_literal_text(&lines[0]));
        output.push_str("\"\"\"\n");
        return;
    }

    output.push_str(&python_docstring_literal_text(&lines[0]));
    output.push('\n');
    for line in lines.iter().skip(1) {
        if !line.is_empty() {
            output.push_str(indent);
            output.push_str(&python_docstring_literal_text(line));
        }
        output.push('\n');
    }
    output.push_str(indent);
    output.push_str("\"\"\"\n");
}

/// Word-wrap one Python docstring line. Mirrors the front-end's
/// `push_wrapped_python_docstring_line`.
fn push_wrapped_python_docstring_line(
    lines: &mut Vec<String>,
    first_prefix: &str,
    continuation_prefix: &str,
    text: &str,
    max_width: usize,
) {
    if text.is_empty() {
        lines.push(first_prefix.trim_end().to_string());
        return;
    }

    let mut prefix = first_prefix;
    let mut current = String::new();
    for word in text.split_whitespace() {
        let prefix_width = prefix.chars().count();
        let current_width = current.chars().count();
        let word_width = word.chars().count();
        let separator_width = usize::from(!current.is_empty());
        if current_width > 0
            && prefix_width + current_width + separator_width + word_width > max_width
        {
            lines.push(format!("{prefix}{current}"));
            prefix = continuation_prefix;
            current.clear();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }

    lines.push(format!("{prefix}{current}"));
}

/// Escape docstring body text. Mirrors the front-end's
/// `python_docstring_literal_text`.
fn python_docstring_literal_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace("\"\"\"", "\\\"\\\"\\\"")
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
    // No imports to render is an empty block in every language — short-circuit
    // before the per-language rendering so emitters that inline their own
    // imports (and declare no refs) never require language-specific support.
    if imports.is_empty() {
        return String::new();
    }
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
