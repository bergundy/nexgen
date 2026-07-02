//! The TypeScript emitter.
//!
//! Renders a single-file `index.ts`: the shared validator core, generated
//! constants (const values + advisory defaults), the model `interface`s,
//! per-type `parse`/`serialize` functions, and the Nexus service binding.

use std::fmt::Write as _;
use std::path::PathBuf;

use heck::ToShoutySnakeCase;
use nex_gen_core::{Emitter, EmittedFile, IR, Language, Result, Service, SymbolId};

use crate::emit::{self, PackageInfo, as_map, as_record, doc_lines, services, symbol_name, type_symbols};
use crate::ir::{Additional, Field, FieldType, Kind, Record, Scalar};
use crate::naming;

const LANG: Language = Language::TypeScript;

/// Build the TypeScript emitter.
pub fn emitter(pkg: PackageInfo) -> Box<dyn Emitter<Kind>> {
    Box::new(TypeScriptEmitter { pkg })
}

struct TypeScriptEmitter {
    #[allow(dead_code)]
    pkg: PackageInfo,
}

impl Emitter<Kind> for TypeScriptEmitter {
    fn language(&self) -> Language {
        LANG
    }

    fn emit(&self, ir: &IR<Kind>) -> Result<Vec<EmittedFile>> {
        let body = render(ir);
        Ok(vec![EmittedFile {
            path: PathBuf::from("index.ts"),
            body,
        }])
    }
}

fn render(ir: &IR<Kind>) -> String {
    let mut out = String::new();
    writeln!(out, "// {}", emit::BANNER).unwrap();
    out.push('\n');
    out.push_str("import * as nexus from \"nexus-rpc\";\n\n");

    out.push_str(SHARED_CORE);

    render_constants(&mut out, ir);
    render_models(&mut out, ir);
    render_functions(&mut out, ir);

    out.push_str(COLLECT_HELPER);

    render_services(&mut out, ir);
    out
}

// ---------------------------------------------------------------------------
// Constants (const values + advisory defaults).
// ---------------------------------------------------------------------------

fn render_constants(out: &mut String, ir: &IR<Kind>) {
    let mut consts: Vec<(String, String, String)> = Vec::new(); // (type, field_json, value)
    let mut defaults: Vec<(String, String, String)> = Vec::new();
    for symbol in type_symbols(ir) {
        let Some(record) = as_record(symbol) else {
            continue;
        };
        let type_name = naming::type_ident(symbol.name.as_str(), LANG, &Default::default());
        for field in &record.fields {
            if let Some(c) = &field.constant {
                consts.push((type_name.clone(), field.json_name.clone(), ts_literal(c)));
            }
            if let Some(d) = &field.default {
                defaults.push((type_name.clone(), field.json_name.clone(), ts_literal(d)));
            }
        }
    }
    if consts.is_empty() && defaults.is_empty() {
        return;
    }
    section(out, "Generated constants (const values + advisory defaults).");
    for (type_name, field, value) in &consts {
        writeln!(
            out,
            "/** `{type_name}.{field}` const value (the wire discriminator). */"
        )
        .unwrap();
        writeln!(out, "const {}_CONST = {value};", field.to_shouty_snake_case()).unwrap();
        out.push('\n');
    }
    for (type_name, field, value) in &defaults {
        writeln!(
            out,
            "/** Advisory default for `{type_name}.{field}`. Read via `?? DEFAULT_{}`. */",
            field.to_shouty_snake_case()
        )
        .unwrap();
        writeln!(
            out,
            "export const DEFAULT_{} = {value};",
            field.to_shouty_snake_case()
        )
        .unwrap();
        out.push('\n');
    }
}

// ---------------------------------------------------------------------------
// Model interfaces.
// ---------------------------------------------------------------------------

fn render_models(out: &mut String, ir: &IR<Kind>) {
    section(out, "Models.");
    for symbol in type_symbols(ir) {
        let type_name = naming::type_ident(symbol.name.as_str(), LANG, &Default::default());
        if let Some(record) = as_record(symbol) {
            render_interface(out, ir, &type_name, record);
        } else if let Some(map) = as_map(symbol) {
            if let Some(docs) = &map.docs {
                jsdoc(out, "", docs);
            }
            writeln!(out, "export interface {type_name} {{").unwrap();
            writeln!(
                out,
                "  additionalProperties: Record<string, {}>;",
                ts_type(&map.value, ir)
            )
            .unwrap();
            out.push_str("}\n\n");
        }
    }
}

fn render_interface(out: &mut String, ir: &IR<Kind>, type_name: &str, record: &Record) {
    if let Some(docs) = &record.docs {
        jsdoc(out, "", docs);
    }
    writeln!(out, "export interface {type_name} {{").unwrap();
    for field in &record.fields {
        if let Some(docs) = &field.docs {
            jsdoc(out, "  ", docs);
        }
        let ident = naming::field_ident(&field.json_name, LANG, &field.overrides);
        writeln!(out, "  {};", interface_field(&ident, field, ir)).unwrap();
    }
    match &record.additional {
        Additional::Open => {
            out.push_str(
                "  /** Unknown members are preserved here so a newer producer never breaks us. */\n",
            );
            out.push_str("  additionalProperties: Record<string, unknown>;\n");
        }
        Additional::Typed(ty) => {
            writeln!(
                out,
                "  additionalProperties: Record<string, {}>;",
                ts_type(ty, ir)
            )
            .unwrap();
        }
        Additional::Closed => {}
    }
    out.push_str("}\n\n");
}

/// Render an interface field declaration (without a trailing `;`).
fn interface_field(ident: &str, field: &Field, ir: &IR<Kind>) -> String {
    if let Some(Scalar::String(v)) = &field.constant {
        return format!("readonly {ident}: \"{v}\" | (string & {{}})");
    }
    let base = ts_type(&field.ty, ir);
    let value = if field.nullable {
        format!("{base} | null")
    } else {
        base
    };
    if field.required {
        format!("{ident}: {value}")
    } else {
        format!("{ident}?: {value}")
    }
}

// ---------------------------------------------------------------------------
// parse / serialize functions.
// ---------------------------------------------------------------------------

fn render_functions(out: &mut String, ir: &IR<Kind>) {
    for symbol in type_symbols(ir) {
        let type_name = naming::type_ident(symbol.name.as_str(), LANG, &Default::default());
        if let Some(record) = as_record(symbol) {
            section(out, &format!("{type_name}."));
            if matches!(record.additional, Additional::Open) {
                let declared: Vec<String> = record
                    .fields
                    .iter()
                    .map(|f| format!("\"{}\"", f.json_name))
                    .collect();
                writeln!(
                    out,
                    "const {}_DECLARED = new Set([{}]);\n",
                    type_name.to_shouty_snake_case(),
                    declared.join(", ")
                )
                .unwrap();
            }
            render_parse_record(out, ir, &type_name, record);
            render_serialize_record(out, ir, &type_name, record);
        } else if let Some(map) = as_map(symbol) {
            section(out, &format!("{type_name} (typed map)."));
            render_parse_map(out, ir, &type_name, map);
            render_serialize_map(out, &type_name);
        }
    }
}

fn render_parse_record(out: &mut String, ir: &IR<Kind>, type_name: &str, record: &Record) {
    writeln!(out, "export function parse{type_name}(raw: unknown): {type_name} {{").unwrap();
    out.push_str("  const violations: Violation[] = [];\n");
    out.push_str("  if (!isPlainObject(raw)) {\n");
    out.push_str("    throw new ValidationError([{ path: \"\", reason: \"expected object\" }]);\n");
    out.push_str("  }\n\n");

    for field in &record.fields {
        let ident = naming::field_ident(&field.json_name, LANG, &field.overrides);
        parse_field(out, ir, &ident, field);
        out.push('\n');
    }

    match &record.additional {
        Additional::Closed => {
            let checks: Vec<String> = record
                .fields
                .iter()
                .map(|f| format!("k !== \"{}\"", f.json_name))
                .collect();
            out.push_str("  // Closed struct: reject unknown keys.\n");
            out.push_str("  for (const k of Object.keys(raw)) {\n");
            if checks.is_empty() {
                out.push_str("    violations.push({ path: k, reason: \"unknown field\" });\n");
            } else {
                writeln!(out, "    if ({}) violations.push({{ path: k, reason: \"unknown field\" }});", checks.join(" && ")).unwrap();
            }
            out.push_str("  }\n\n");
        }
        Additional::Open => {
            out.push_str("  // Open struct: preserve unknown members in the catch-all.\n");
            out.push_str("  const additionalProperties: Record<string, unknown> = {};\n");
            writeln!(
                out,
                "  for (const k of Object.keys(raw)) {{\n    if (!{}_DECLARED.has(k)) additionalProperties[k] = raw[k];\n  }}\n",
                type_name.to_shouty_snake_case()
            )
            .unwrap();
        }
        Additional::Typed(ty) => {
            let declared: Vec<String> = record
                .fields
                .iter()
                .map(|f| format!("k !== \"{}\"", f.json_name))
                .collect();
            out.push_str("  // Typed additionalProperties.\n");
            out.push_str("  const additionalProperties: Record<string, ");
            out.push_str(&ts_type(ty, ir));
            out.push_str("> = {};\n");
            out.push_str("  for (const k of Object.keys(raw)) {\n");
            let cond = if declared.is_empty() {
                "true".to_string()
            } else {
                declared.join(" && ")
            };
            writeln!(out, "    if ({cond}) additionalProperties[k] = raw[k] as {};", ts_type(ty, ir)).unwrap();
            out.push_str("  }\n\n");
        }
    }

    out.push_str("  if (violations.length) throw new ValidationError(violations);\n");
    // Build the return object.
    let has_open = !matches!(record.additional, Additional::Closed);
    let (required, optional): (Vec<&Field>, Vec<&Field>) = record
        .fields
        .iter()
        .partition(|f| f.required || f.constant.is_some());
    let mut base: Vec<String> = required
        .iter()
        .map(|f| naming::field_ident(&f.json_name, LANG, &f.overrides))
        .collect();
    if has_open {
        base.push("additionalProperties".to_string());
    }
    if optional.is_empty() {
        writeln!(out, "  return {{ {} }};", base.join(", ")).unwrap();
    } else {
        writeln!(out, "  const out: {type_name} = {{ {} }};", base.join(", ")).unwrap();
        for field in &optional {
            let ident = naming::field_ident(&field.json_name, LANG, &field.overrides);
            writeln!(out, "  if ({ident} !== undefined) out.{ident} = {ident};").unwrap();
        }
        out.push_str("  return out;\n");
    }
    out.push_str("}\n\n");
}

/// Emit the per-field parse block into `parse<T>`.
fn parse_field(out: &mut String, ir: &IR<Kind>, ident: &str, field: &Field) {
    let json = &field.json_name;
    // const string field.
    if let Some(Scalar::String(_)) = &field.constant {
        let cname = format!("{}_CONST", json.to_shouty_snake_case());
        writeln!(out, "  // {json}: required const.").unwrap();
        writeln!(out, "  let {ident}: string = {cname};").unwrap();
        writeln!(out, "  if (raw.{json} === undefined || raw.{json} === null) {{").unwrap();
        writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"required\" }});").unwrap();
        writeln!(out, "  }} else if (typeof raw.{json} !== \"string\") {{").unwrap();
        writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"expected string\" }});").unwrap();
        writeln!(out, "  }} else if (raw.{json} !== {cname}) {{").unwrap();
        writeln!(out, "    violations.push({{ path: \"{json}\", reason: `must equal \"${{{cname}}}\"` }});").unwrap();
        writeln!(out, "  }} else {{").unwrap();
        writeln!(out, "    {ident} = raw.{json};").unwrap();
        out.push_str("  }\n");
        return;
    }

    match &field.ty {
        FieldType::Ref(id) => {
            let type_name = ref_type(ir, *id);
            if field.required {
                writeln!(out, "  // {json}: required {type_name}.").unwrap();
                writeln!(out, "  let {ident}: {type_name} = undefined as unknown as {type_name};").unwrap();
                writeln!(out, "  if (raw.{json} === undefined || raw.{json} === null) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"required\" }});").unwrap();
                writeln!(out, "  }} else {{").unwrap();
                writeln!(out, "    try {{ {ident} = parse{type_name}(raw.{json}); }} catch (e) {{ collect(violations, \"{json}\", e); }}").unwrap();
                out.push_str("  }\n");
            } else {
                writeln!(out, "  // {json}: optional {type_name}.").unwrap();
                writeln!(out, "  let {ident}: {type_name} | undefined = undefined;").unwrap();
                writeln!(out, "  if (raw.{json} === null) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"explicit null not allowed\" }});").unwrap();
                writeln!(out, "  }} else if (raw.{json} !== undefined) {{").unwrap();
                writeln!(out, "    try {{ {ident} = parse{type_name}(raw.{json}); }} catch (e) {{ collect(violations, \"{json}\", e); }}").unwrap();
                out.push_str("  }\n");
            }
        }
        FieldType::Array(inner) => {
            let elem = ts_type(inner, ir);
            writeln!(out, "  // {json}: optional {elem}[].").unwrap();
            writeln!(out, "  let {ident}: {elem}[] | undefined = undefined;").unwrap();
            writeln!(out, "  if (raw.{json} === null) {{").unwrap();
            writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"explicit null not allowed\" }});").unwrap();
            writeln!(out, "  }} else if (raw.{json} !== undefined) {{").unwrap();
            writeln!(out, "    if (!Array.isArray(raw.{json})) {{").unwrap();
            writeln!(out, "      violations.push({{ path: \"{json}\", reason: \"expected array\" }});").unwrap();
            writeln!(out, "    }} else {{").unwrap();
            writeln!(out, "      {ident} = [];").unwrap();
            writeln!(out, "      (raw.{json} as unknown[]).forEach((el, i) => {{").unwrap();
            let (check, _) = primitive_check(inner);
            writeln!(out, "        if ({}) {{", check.replace("VAR", "el")).unwrap();
            writeln!(out, "          violations.push({{ path: `{json}[${{i}}]`, reason: \"expected element\" }});").unwrap();
            writeln!(out, "        }} else {{ {ident}!.push(el as {elem}); }}").unwrap();
            out.push_str("      });\n");
            out.push_str("    }\n");
            out.push_str("  }\n");
        }
        primitive => {
            let (check, tyname) = primitive_check(primitive);
            // `tyname` is the human-readable word for messages ("integer"); the
            // TS type annotation must be a real TS type ("number").
            let annot = ts_type(primitive, ir);
            let check = check.replace("VAR", &format!("raw.{json}"));
            if field.required && field.nullable {
                writeln!(out, "  // {json}: required + nullable.").unwrap();
                writeln!(out, "  let {ident}: {annot} | null = undefined as unknown as {annot} | null;").unwrap();
                writeln!(out, "  if (raw.{json} === undefined) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"required\" }});").unwrap();
                writeln!(out, "  }} else if (raw.{json} === null) {{").unwrap();
                writeln!(out, "    {ident} = null;").unwrap();
                writeln!(out, "  }} else if ({check}) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"expected {tyname}\" }});").unwrap();
                writeln!(out, "  }} else {{ {ident} = raw.{json}; }}").unwrap();
            } else if field.required {
                writeln!(out, "  // {json}: required {tyname}.").unwrap();
                writeln!(out, "  let {ident}: {annot} = undefined as unknown as {annot};").unwrap();
                writeln!(out, "  if (raw.{json} === undefined || raw.{json} === null) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"required\" }});").unwrap();
                writeln!(out, "  }} else if ({check}) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"expected {tyname}\" }});").unwrap();
                writeln!(out, "  }} else {{ {ident} = raw.{json}; }}").unwrap();
            } else if field.nullable {
                writeln!(out, "  // {json}: optional + nullable {tyname}.").unwrap();
                writeln!(out, "  let {ident}: {annot} | null | undefined = undefined;").unwrap();
                writeln!(out, "  if (raw.{json} === null) {{ {ident} = null; }}").unwrap();
                writeln!(out, "  else if (raw.{json} !== undefined) {{").unwrap();
                writeln!(out, "    if ({check}) violations.push({{ path: \"{json}\", reason: \"expected {tyname}\" }});").unwrap();
                writeln!(out, "    else {ident} = raw.{json};").unwrap();
                out.push_str("  }\n");
            } else {
                writeln!(out, "  // {json}: optional {tyname}.").unwrap();
                writeln!(out, "  let {ident}: {annot} | undefined = undefined;").unwrap();
                writeln!(out, "  if (raw.{json} === null) {{").unwrap();
                writeln!(out, "    violations.push({{ path: \"{json}\", reason: \"explicit null not allowed\" }});").unwrap();
                writeln!(out, "  }} else if (raw.{json} !== undefined) {{").unwrap();
                writeln!(out, "    if ({check}) violations.push({{ path: \"{json}\", reason: \"expected {tyname}\" }});").unwrap();
                writeln!(out, "    else {ident} = raw.{json};").unwrap();
                out.push_str("  }\n");
            }
        }
    }
}

fn render_serialize_record(out: &mut String, ir: &IR<Kind>, type_name: &str, record: &Record) {
    writeln!(out, "export function serialize{type_name}(value: {type_name}): unknown {{").unwrap();
    out.push_str("  const out: Record<string, unknown> = {};\n");
    for field in &record.fields {
        let ident = naming::field_ident(&field.json_name, LANG, &field.overrides);
        let json = &field.json_name;
        if field.constant.is_some() || (field.required && !field.nullable) {
            match &field.ty {
                FieldType::Ref(id) => writeln!(out, "  out.{json} = serialize{}(value.{ident});", ref_type(ir, *id)).unwrap(),
                _ => writeln!(out, "  out.{json} = value.{ident};").unwrap(),
            }
        } else if field.required && field.nullable {
            writeln!(out, "  out.{json} = value.{ident}; // required + nullable: always emitted").unwrap();
        } else {
            match &field.ty {
                FieldType::Ref(id) => writeln!(out, "  if (value.{ident} !== undefined) out.{json} = serialize{}(value.{ident});", ref_type(ir, *id)).unwrap(),
                _ => writeln!(out, "  if (value.{ident} !== undefined) out.{json} = value.{ident};").unwrap(),
            }
        }
    }
    if !matches!(record.additional, Additional::Closed) {
        out.push_str("  for (const [k, v] of Object.entries(value.additionalProperties ?? {})) out[k] = v;\n");
    }
    out.push_str("  return out;\n");
    out.push_str("}\n\n");
}

fn render_parse_map(out: &mut String, ir: &IR<Kind>, type_name: &str, map: &crate::ir::MapType) {
    let value_ty = ts_type(&map.value, ir);
    writeln!(out, "export function parse{type_name}(raw: unknown): {type_name} {{").unwrap();
    out.push_str("  const violations: Violation[] = [];\n");
    out.push_str("  if (!isPlainObject(raw)) {\n");
    out.push_str("    throw new ValidationError([{ path: \"\", reason: \"expected object\" }]);\n");
    out.push_str("  }\n");
    out.push_str("  const keys = Object.keys(raw);\n");
    if let Some(max) = map.max_properties {
        writeln!(out, "  if (keys.length > {max}) {{").unwrap();
        writeln!(out, "    violations.push({{ path: \"\", reason: \"maxProperties: at most {max} entries\" }});").unwrap();
        out.push_str("  }\n");
    }
    if let Some(min) = map.min_properties {
        writeln!(out, "  if (keys.length < {min}) {{").unwrap();
        writeln!(out, "    violations.push({{ path: \"\", reason: \"minProperties: at least {min} entries\" }});").unwrap();
        out.push_str("  }\n");
    }
    writeln!(out, "  const additionalProperties: Record<string, {value_ty}> = {{}};").unwrap();
    let (check, tyname) = primitive_check(&map.value);
    let check = check.replace("VAR", "raw[k]");
    out.push_str("  for (const k of keys) {\n");
    writeln!(out, "    if ({check}) {{").unwrap();
    writeln!(out, "      violations.push({{ path: k, reason: \"expected {tyname}\" }});").unwrap();
    writeln!(out, "    }} else {{ additionalProperties[k] = raw[k] as {value_ty}; }}").unwrap();
    out.push_str("  }\n");
    out.push_str("  if (violations.length) throw new ValidationError(violations);\n");
    out.push_str("  return { additionalProperties };\n");
    out.push_str("}\n\n");
}

fn render_serialize_map(out: &mut String, type_name: &str) {
    writeln!(out, "export function serialize{type_name}(value: {type_name}): unknown {{").unwrap();
    out.push_str("  const out: Record<string, unknown> = {};\n");
    out.push_str("  for (const [k, v] of Object.entries(value.additionalProperties ?? {})) out[k] = v;\n");
    out.push_str("  return out;\n");
    out.push_str("}\n\n");
}

// ---------------------------------------------------------------------------
// Service binding.
// ---------------------------------------------------------------------------

fn render_services(out: &mut String, ir: &IR<Kind>) {
    let services = services(ir);
    if services.is_empty() {
        return;
    }
    section(out, "Service binding.");
    for service in services {
        render_service(out, ir, service);
    }
}

fn render_service(out: &mut String, ir: &IR<Kind>, service: &Service) {
    if let Some(docs) = &service.docs {
        jsdoc(out, "", docs);
    }
    let const_name = naming::service_ident(service.name.as_str(), LANG);
    writeln!(
        out,
        "export const {const_name} = nexus.service(\"{}\", {{",
        service.wire_name
    )
    .unwrap();
    for op in &service.operations {
        if let Some(docs) = &op.docs {
            jsdoc(out, "  ", docs);
        }
        let attr = naming::field_ident(op.name.as_str(), LANG, &Default::default());
        let input = io_type(ir, op.input);
        let output = io_type(ir, op.output);
        writeln!(out, "  {attr}: nexus.operation<{input}, {output}>({{").unwrap();
        writeln!(out, "    name: \"{}\",", op.wire_name).unwrap();
        out.push_str("  }),\n");
    }
    out.push_str("});\n");
}

fn io_type(ir: &IR<Kind>, id: Option<SymbolId>) -> String {
    match id {
        Some(id) => ref_type(ir, id),
        None => "void".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn ref_type(ir: &IR<Kind>, id: SymbolId) -> String {
    naming::type_ident(symbol_name(ir, id), LANG, &Default::default())
}

fn ts_type(ty: &FieldType, ir: &IR<Kind>) -> String {
    match ty {
        FieldType::String => "string".to_string(),
        FieldType::Integer | FieldType::Number => "number".to_string(),
        FieldType::Boolean => "boolean".to_string(),
        FieldType::Array(inner) => format!("{}[]", ts_type(inner, ir)),
        FieldType::Ref(id) => ref_type(ir, *id),
    }
}

/// A `typeof`-style rejection check with `VAR` as the placeholder, plus the TS
/// type name. The check evaluates to `true` when the value is the WRONG type.
fn primitive_check(ty: &FieldType) -> (String, String) {
    match ty {
        FieldType::String => ("typeof VAR !== \"string\"".to_string(), "string".to_string()),
        FieldType::Integer => (
            "typeof VAR !== \"number\" || !Number.isSafeInteger(VAR)".to_string(),
            "integer".to_string(),
        ),
        FieldType::Number => ("typeof VAR !== \"number\"".to_string(), "number".to_string()),
        FieldType::Boolean => ("typeof VAR !== \"boolean\"".to_string(), "boolean".to_string()),
        FieldType::Array(_) => ("!Array.isArray(VAR)".to_string(), "array".to_string()),
        FieldType::Ref(_) => ("false".to_string(), "object".to_string()),
    }
}

fn ts_literal(scalar: &Scalar) -> String {
    match scalar {
        Scalar::String(s) => format!("\"{s}\""),
        Scalar::Integer(i) => i.to_string(),
        Scalar::Number(n) => n.to_string(),
        Scalar::Boolean(b) => b.to_string(),
    }
}

fn section(out: &mut String, title: &str) {
    out.push_str("// ---------------------------------------------------------------------------\n");
    writeln!(out, "// {title}").unwrap();
    out.push_str("// ---------------------------------------------------------------------------\n\n");
}

/// Emit a JSDoc block at the given indent from a doc string.
fn jsdoc(out: &mut String, indent: &str, docs: &str) {
    let lines = doc_lines(docs);
    if lines.is_empty() {
        return;
    }
    writeln!(out, "{indent}/**").unwrap();
    for line in &lines {
        if line.is_empty() {
            writeln!(out, "{indent} *").unwrap();
        } else {
            writeln!(out, "{indent} * {line}").unwrap();
        }
    }
    writeln!(out, "{indent} */").unwrap();
}

const SHARED_CORE: &str = r#"// ---------------------------------------------------------------------------
// Shared validator core (emitted once per package; inline for single-input).
// ---------------------------------------------------------------------------

/** A single constraint failure, located by JSON path. */
export interface Violation {
  readonly path: string;
  readonly reason: string;
}

/**
 * Aggregates every {@link Violation} found while (de)serializing a value
 * and surfaces them all in one error — never a partial first-failure.
 * Mirrors Java's `ValidationException` / `List<Violation>` and Python's
 * `pydantic.ValidationError`.
 */
export class ValidationError extends Error {
  constructor(readonly violations: Violation[]) {
    super(
      `${violations.length} validation error(s): ` +
        violations.map((v) => `${v.path}: ${v.reason}`).join("; "),
    );
    this.name = "ValidationError";
  }
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

"#;

const COLLECT_HELPER: &str = r#"/** Re-aggregate a nested ValidationError's violations under a parent path. */
function collect(violations: Violation[], path: string, e: unknown): void {
  if (e instanceof ValidationError) {
    for (const inner of e.violations) {
      violations.push({ path: `${path}.${inner.path}`, reason: inner.reason });
    }
  } else {
    violations.push({ path, reason: String(e) });
  }
}

"#;
