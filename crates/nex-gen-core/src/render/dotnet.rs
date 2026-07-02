//! .NET (C#) service and import rendering.

use heck::ToUpperCamelCase;

use super::NameResolver;
use crate::ir::{Operation, Service, SymbolId};

/// A resolved .NET import: a whole-namespace `using <module>;`.
///
/// C# generated code has no named or aliased import and no type-only import, so
/// the module namespace is all that is needed.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Import {
    /// The namespace being imported (`using <module>;`).
    pub module: String,
}

/// Render a .NET `using` block for a file's [`Import`]s.
///
/// Order is preserved from `imports` — the front-end supplies its `using`s in
/// dependency order, and unlike Python/TypeScript the .NET output is not run
/// through a formatter, so it is not re-sorted (or deduped) here.
pub fn render_imports(imports: &[Import]) -> String {
    imports
        .iter()
        .map(|import| format!("using {};", import.module))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `[GeneratedCode]` attribute stamped on every generated declaration.
/// Mirrors the front-end's `GENERATED_CODE_ATTRIBUTE`.
const GENERATED_CODE_ATTRIBUTE: &str = "[GeneratedCode(\"nex-gen\", null)]";

/// The experimental-warning text emitted in the `<remarks>` XML doc tag. The
/// .NET wording differs from the shared [`EXPERIMENTAL_WARNING`](super::EXPERIMENTAL_WARNING),
/// so it is kept local. Mirrors the front-end's `EXPERIMENTAL_WARNING`.
const EXPERIMENTAL_WARNING: &str =
    "WARNING: This API is experimental and may change in the future.";

/// Render the .NET Nexus service binding: the `[NexusService(...)]`-attributed
/// `internal interface I<Name>` whose methods are the `[NexusOperation(...)]`
/// operations.
///
/// Operation I/O type names come from `names` (`type_ref`); the front-end
/// adapter registers the already-resolved C# type strings. The file prelude,
/// `using` block, and namespace wrapping stay in the front-end.
pub fn render_service(svc: &Service, names: &dyn NameResolver) -> String {
    let mut output = String::new();
    render_xml_summary(&mut output, "", svc.docs.as_deref(), svc.experimental);
    output.push_str(GENERATED_CODE_ATTRIBUTE);
    output.push('\n');
    output.push_str("[NexusService(");
    output.push_str(&string_literal(&svc.wire_name));
    output.push_str(")]\n");
    output.push_str("internal interface I");
    output.push_str(&type_name(svc.name.as_str()));
    output.push_str("\n{\n");
    for operation in &svc.operations {
        render_service_operation(&mut output, operation, names);
    }
    output.push_str("}\n\n");
    output
}

/// Render one interface method for `operation`. The request parameter is
/// emitted only when the operation has an input (`input.is_some()`); a `None`
/// output renders `void`.
fn render_service_operation(output: &mut String, operation: &Operation, names: &dyn NameResolver) {
    render_operation_xml_doc(output, "    ", operation);
    output.push_str("    ");
    output.push_str(GENERATED_CODE_ATTRIBUTE);
    output.push('\n');
    output.push_str("    [NexusOperation(");
    output.push_str(&string_literal(&operation.wire_name));
    output.push_str(")]\n");
    output.push_str("    ");
    output.push_str(&operation_return_type(operation.output, names));
    output.push(' ');
    output.push_str(&type_name(operation.name.as_str()));
    output.push('(');
    if let Some(input) = operation.input {
        output.push_str(&names.type_ref(input));
        output.push_str(" request");
    }
    output.push_str(");\n\n");
}

/// Resolve an operation output symbol to its C# return type, rendering `void`
/// when the operation has no output. Mirrors the front-end's
/// `operation_raw_return_type` (the `None` -> `"void"` collapse included).
fn operation_return_type(id: Option<SymbolId>, names: &dyn NameResolver) -> String {
    match id {
        Some(id) => names.type_ref(id),
        None => "void".to_string(),
    }
}

/// Render the interface method's XML doc: summary + `<returns>` + experimental
/// `<remarks>`. Mirrors the front-end's `render_operation_summary_xml_doc`; the
/// front-end resolves `returns_doc` to `None` for output-less operations.
fn render_operation_xml_doc(output: &mut String, indent: &str, operation: &Operation) {
    render_xml_doc(
        output,
        indent,
        operation.docs.as_deref(),
        Vec::new(),
        operation.returns_doc.as_deref(),
        operation.experimental,
    );
}

/// Render a `<summary>`-only XML doc block. Mirrors the front-end's
/// `render_xml_summary`.
fn render_xml_summary(
    output: &mut String,
    indent: &str,
    summary: Option<&str>,
    experimental: bool,
) {
    render_xml_doc(output, indent, summary, Vec::new(), None, experimental);
}

/// Render an XML doc comment from an optional summary, params, returns, and the
/// experimental flag. Mirrors the front-end's `render_xml_doc`.
fn render_xml_doc(
    output: &mut String,
    indent: &str,
    summary: Option<&str>,
    params: Vec<(String, String)>,
    returns: Option<&str>,
    experimental: bool,
) {
    if summary.is_none() && params.is_empty() && returns.is_none() && !experimental {
        return;
    }
    if let Some(summary) = summary {
        output.push_str(indent);
        output.push_str("/// <summary>\n");
        render_xml_doc_text(output, indent, summary);
        output.push_str(indent);
        output.push_str("/// </summary>\n");
    }
    for (name, doc) in params {
        output.push_str(indent);
        output.push_str("/// <param name=\"");
        output.push_str(&name);
        output.push_str("\">");
        output.push_str(&xml_doc_escape(doc.trim()));
        output.push_str("</param>\n");
    }
    if let Some(returns) = returns {
        output.push_str(indent);
        output.push_str("/// <returns>");
        output.push_str(&xml_doc_escape(returns.trim()));
        output.push_str("</returns>\n");
    }
    if experimental {
        output.push_str(indent);
        output.push_str("/// <remarks>");
        output.push_str(EXPERIMENTAL_WARNING);
        output.push_str("</remarks>\n");
    }
}

/// Emit `/// `-prefixed, escaped doc-text lines. Mirrors the front-end's
/// `render_xml_doc_text`.
fn render_xml_doc_text(output: &mut String, indent: &str, text: &str) {
    for line in text.trim().lines() {
        output.push_str(indent);
        output.push_str("/// ");
        output.push_str(&xml_doc_escape(line.trim()));
        output.push('\n');
    }
}

/// Escape XML doc text. Mirrors the front-end's `xml_doc_escape`.
fn xml_doc_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The C# type name for a canonical name: upper-camel-cased and escaped against
/// C# keywords. Mirrors the front-end's `csharp_type_name`.
fn type_name(name: &str) -> String {
    ident(&name.to_upper_camel_case())
}

/// Escape an identifier that is empty, starts with a non-identifier character,
/// or collides with a C# keyword. Mirrors the front-end's `csharp_ident`.
fn ident(name: &str) -> String {
    let candidate = if name.is_empty() {
        "Value".to_string()
    } else if name
        .chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
    {
        name.to_string()
    } else {
        format!("_{name}")
    };
    if CSHARP_KEYWORDS.contains(&candidate.as_str()) {
        format!("@{candidate}")
    } else {
        candidate
    }
}

/// A C# string literal. Mirrors the front-end's `csharp_string_literal`.
fn string_literal(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
    )
}

const CSHARP_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "base",
    "bool",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "checked",
    "class",
    "const",
    "continue",
    "decimal",
    "default",
    "delegate",
    "do",
    "double",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "false",
    "finally",
    "fixed",
    "float",
    "for",
    "foreach",
    "goto",
    "if",
    "implicit",
    "in",
    "int",
    "interface",
    "internal",
    "is",
    "lock",
    "long",
    "namespace",
    "new",
    "null",
    "object",
    "operator",
    "out",
    "override",
    "params",
    "private",
    "protected",
    "public",
    "readonly",
    "ref",
    "return",
    "sbyte",
    "sealed",
    "short",
    "sizeof",
    "stackalloc",
    "static",
    "string",
    "struct",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "uint",
    "ulong",
    "unchecked",
    "unsafe",
    "ushort",
    "using",
    "virtual",
    "void",
    "volatile",
    "while",
];
