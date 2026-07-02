//! Python service and import rendering.

use std::collections::{BTreeMap, BTreeSet};

use heck::ToSnakeCase;

use super::{EXPERIMENTAL_WARNING, NameResolver};
use crate::ir::{Service, SymbolId};

/// A resolved Python import.
///
/// Python has only two shapes, and no type-only imports:
///
/// - [`Module`](Import::Module) — `import <module>` (whole-module import; uses
///   are qualified through the module path).
/// - [`Named`](Import::Named) — `from <module> import <name>`, merged per module.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Import {
    /// `import <module>`.
    Module { module: String },
    /// `from <module> import <name>`, merged per module.
    Named { module: String, name: String },
}

/// Python output line length used for docstring wrapping. Mirrors the
/// front-end crate's `PYTHON_FORMAT_LINE_LENGTH`.
const FORMAT_LINE_LENGTH: usize = 88;

/// Render the Python Nexus service binding (`@service` decorated `class` whose
/// operations are `Operation[In, Out]` class attributes).
///
/// Operation I/O type names come from `names` (`type_ref`); the front-end
/// adapter passes already-resolved (and Python-placement-stripped) refs.
pub fn render_service(svc: &Service, names: &dyn NameResolver) -> String {
    let mut output = String::new();
    if svc.wire_name == svc.name.as_str() {
        output.push_str("@service\n");
    } else {
        output.push_str("@service(name=");
        output.push_str(&string_literal(&svc.wire_name));
        output.push_str(")\n");
    }
    output.push_str("class ");
    output.push_str(svc.name.as_str());
    output.push_str(":\n");
    render_docstring(&mut output, "    ", None, &[], None, svc.experimental);

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
        output.push_str(&operation_attr_name(operation.name.as_str()));
        output.push_str(": Operation[\n");
        output.push_str("        ");
        output.push_str(&operation_io_ref(operation.input, names));
        output.push_str(",\n");
        output.push_str("        ");
        output.push_str(&operation_io_ref(operation.output, names));
        output.push_str(",\n");
        output.push_str("    ] = Operation(name=");
        output.push_str(&string_literal(&operation.wire_name));
        output.push_str(")\n");

        if operation_index + 1 != svc.operations.len() {
            output.push('\n');
        }
    }
    output
}

/// Resolve an operation I/O symbol to its Python type name. Frontends supply
/// both refs as concrete types (Python's `Operation[...]` lists both, with no
/// `void`/`None` collapsing), so `None` never occurs for current inputs; we
/// fall back to `None` (Python's no-value type name) defensively.
fn operation_io_ref(id: Option<SymbolId>, names: &dyn NameResolver) -> String {
    match id {
        Some(id) => names.type_ref(id),
        None => "None".to_string(),
    }
}

/// The attribute name an operation is bound under on the service class: the
/// canonical name snake-cased and escaped against Python keywords. Mirrors the
/// front-end's `python_ident(name.to_snake_case())`.
fn operation_attr_name(name: &str) -> String {
    ident(&name.to_snake_case())
}

/// Escape a name that collides with a Python keyword by suffixing `_`. Mirrors
/// the front-end's `python_ident`.
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
fn string_literal(value: &str) -> String {
    format!("{value:?}")
}

/// Render a Python docstring from an optional summary, args, returns, and the
/// experimental flag. Mirrors the front-end's `render_python_docstring`. The
/// service-binding path only ever passes `summary = None`, no args, no returns,
/// and an `experimental` flag, but the full logic is ported byte-for-byte.
fn render_docstring(
    output: &mut String,
    indent: &str,
    summary: Option<&str>,
    args: &[(String, String)],
    returns: Option<&str>,
    experimental: bool,
) {
    let mut lines = Vec::<String>::new();
    let docstring_width = FORMAT_LINE_LENGTH.saturating_sub(indent.chars().count());
    let has_summary = summary.is_some_and(|summary| !summary.trim().is_empty());
    if let Some(summary) = summary.map(str::trim).filter(|summary| !summary.is_empty()) {
        for line in summary.lines() {
            push_wrapped_docstring_line(&mut lines, "", "", line.trim(), docstring_width);
        }
    }
    if experimental {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(".. warning::".to_string());
        push_wrapped_docstring_line(
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
            push_wrapped_docstring_line(
                &mut lines,
                &first_prefix,
                continuation_prefix,
                first,
                docstring_width,
            );
            for line in doc_lines {
                push_wrapped_docstring_line(
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
            push_wrapped_docstring_line(&mut lines, "    ", "    ", first.trim(), docstring_width);
        }
        for line in return_lines {
            push_wrapped_docstring_line(&mut lines, "    ", "    ", line.trim(), docstring_width);
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
                output.push_str(&docstring_literal_text(line));
            }
            output.push('\n');
        }
        output.push_str(indent);
        output.push_str("\"\"\"\n");
        return;
    }
    if lines.len() == 1 {
        output.push_str(&docstring_literal_text(&lines[0]));
        output.push_str("\"\"\"\n");
        return;
    }

    output.push_str(&docstring_literal_text(&lines[0]));
    output.push('\n');
    for line in lines.iter().skip(1) {
        if !line.is_empty() {
            output.push_str(indent);
            output.push_str(&docstring_literal_text(line));
        }
        output.push('\n');
    }
    output.push_str(indent);
    output.push_str("\"\"\"\n");
}

/// Word-wrap one Python docstring line. Mirrors the front-end's
/// `push_wrapped_python_docstring_line`.
fn push_wrapped_docstring_line(
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
fn docstring_literal_text(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace("\"\"\"", "\\\"\\\"\\\"")
}

/// Render a Python import block.
///
/// - [`Import::Module`] => `import <mod>` (Python imports the whole module path).
/// - [`Import::Named`] => `from <mod> import (X, Y, ...)`, names merged per
///   module and sorted.
pub fn render_imports(imports: &[Import]) -> String {
    let mut module_imports: BTreeSet<String> = BTreeSet::new();
    let mut named: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for import in imports {
        match import {
            Import::Module { module } => {
                module_imports.insert(module.clone());
            }
            Import::Named { module, name } => {
                named
                    .entry(module.clone())
                    .or_default()
                    .insert(name.clone());
            }
        }
    }

    let mut lines: Vec<String> = Vec::new();
    for module in &module_imports {
        lines.push(format!("import {module}"));
    }
    for (module, names) in &named {
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        lines.push(render_named_import(module, &names));
    }
    lines.join("\n")
}

/// Render one Python `from <module> import (...)` statement.
fn render_named_import(module: &str, names: &[&str]) -> String {
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
