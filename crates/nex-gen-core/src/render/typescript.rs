//! TypeScript service and import rendering.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToLowerCamelCase;

use super::{EXPERIMENTAL_WARNING, NameResolver};
use crate::ir::{Service, SymbolId};

/// A resolved TypeScript import.
///
/// TypeScript has two shapes, either of which can be type-only (`import type`):
///
/// - [`Star`](Import::Star) — `import [type] * as <alias> from "<module>"`
///   (whole-module or namespace-head; referrers qualify through `alias`).
/// - [`Named`](Import::Named) — `import [type] { <name> } from "<module>"`,
///   merged per `(module, type_only)`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Import {
    /// `import [type] * as <alias> from "<module>"`.
    Star {
        module: String,
        alias: String,
        type_only: bool,
    },
    /// `import [type] { <name> } from "<module>"`, merged per `(module, type_only)`.
    Named {
        module: String,
        name: String,
        type_only: bool,
    },
}

/// TypeScript output line length used for doc-comment wrapping. Mirrors the
/// front-end crate's `TYPESCRIPT_FORMAT_LINE_LENGTH`.
const FORMAT_LINE_LENGTH: usize = 88;

/// Render the TypeScript Nexus service binding
/// (`export const X = nexus.service('wire', { op: nexus.operation<In, Out>({ name: ... }) });`).
///
/// Operation I/O type names come from `names` (`type_ref`), never from
/// frontend-specific knowledge.
pub fn render_service(svc: &Service, names: &dyn NameResolver) -> String {
    let mut output = String::new();
    let service_doc_tags = experimental_doc_tag(svc.experimental);
    render_doc_comment(&mut output, "", None, &service_doc_tags);
    output.push_str("export const ");
    output.push_str(svc.name.as_str());
    output.push_str(" = nexus.service('");
    output.push_str(&svc.wire_name);
    output.push_str("', {\n");
    for operation in &svc.operations {
        let operation_doc_tags = experimental_doc_tag(operation.experimental);
        render_doc_comment(&mut output, "  ", None, &operation_doc_tags);
        output.push_str("  ");
        output.push_str(&operation_attr_name(operation.name.as_str()));
        output.push_str(": nexus.operation<\n");
        output.push_str("    ");
        output.push_str(&operation_io_ref(operation.input, names));
        output.push_str(",\n");
        output.push_str("    ");
        output.push_str(&operation_io_ref(operation.output, names));
        output.push('\n');
        output.push_str("  >({ name: ");
        output.push_str(&string_literal(&operation.wire_name));
        output.push_str(" }),\n");
    }
    output.push_str("});\n\n");
    output
}

/// Resolve an operation I/O symbol to its TypeScript type name, defaulting to
/// `void` when the operation has no input/output (matching the front-end's
/// `PlannedOperationOutput::None` -> `"void"`).
fn operation_io_ref(id: Option<SymbolId>, names: &dyn NameResolver) -> String {
    match id {
        Some(id) => names.type_ref(id),
        None => "void".to_string(),
    }
}

/// The attribute name an operation is bound under in the service object: the
/// canonical name lower-camel-cased and escaped against TS keywords. Mirrors
/// the front-end's `typescript_ident(name.to_lower_camel_case())`.
fn operation_attr_name(name: &str) -> String {
    ident(&name.to_lower_camel_case())
}

/// Escape a name that collides with a TypeScript keyword by suffixing `_`.
/// Mirrors the front-end's `typescript_ident`.
fn ident(name: &str) -> String {
    if is_keyword(name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

fn is_keyword(name: &str) -> bool {
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
fn string_literal(value: &str) -> String {
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
fn render_doc_comment(
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
            push_wrapped_doc_line(output, indent, "", "", line.trim());
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
        push_wrapped_doc_line(output, indent, &format!("{tag} "), "  ", doc);
    }
    output.push_str(indent);
    output.push_str(" */\n");
}

/// Word-wrap one JSDoc line. Mirrors the front-end's
/// `push_wrapped_typescript_doc_line`.
fn push_wrapped_doc_line(
    output: &mut String,
    indent: &str,
    first_prefix: &str,
    continuation_prefix: &str,
    text: &str,
) {
    let max_width = FORMAT_LINE_LENGTH.saturating_sub(indent.chars().count() + 3);
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

/// Render a TypeScript import block.
///
/// - [`Import::Star`] => `import [type] * as <alias> from "<mod>"`.
/// - [`Import::Named`] => `import [type] { X, Y } from "<mod>"`, names merged
///   per `(module, type_only)` and sorted.
///
/// `type_only` selects `import type`; type-only and value imports for the same
/// module render as separate statements (they cannot merge).
pub fn render_imports(imports: &[Import]) -> String {
    let mut lines: Vec<String> = Vec::new();

    // Star imports: one statement each, kept in input order's sorted form via a
    // set keyed by the rendered line.
    let mut star_lines: BTreeSet<String> = BTreeSet::new();
    // Named, merged per (module, type_only).
    let mut named: BTreeMap<(String, bool), BTreeSet<String>> = BTreeMap::new();

    for import in imports {
        match import {
            Import::Star {
                module,
                alias,
                type_only,
            } => {
                let type_kw = if *type_only { "type " } else { "" };
                star_lines.insert(format!("import {type_kw}* as {alias} from \"{module}\";"));
            }
            Import::Named {
                module,
                name,
                type_only,
            } => {
                named
                    .entry((module.clone(), *type_only))
                    .or_default()
                    .insert(name.clone());
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
        lines.push(format!("import {type_kw}{{ {joined} }} from \"{module}\";"));
    }
    lines.join("\n")
}
