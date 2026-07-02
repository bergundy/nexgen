//! The Go emitter.
//!
//! Renders one `<package>.go` file: the banner, the shared validator core
//! (fixed boilerplate), the Nexus service binding, `const`-discriminator type
//! aliases, and one struct + `UnmarshalJSON`/`MarshalJSON`/`Validate` per type.

use std::fmt::Write;
use std::path::PathBuf;

use heck::ToPascalCase;
use nex_gen_core::{Emitter, EmittedFile, IR, Language, Result, Service, SymbolId};

use crate::emit::{BANNER, PackageInfo, as_map, as_record, services, symbol_name, type_symbols};
use crate::ir::{Additional, Field, FieldType, Kind, MapType, NameOverrides, Record, Scalar};
use crate::naming;

pub fn emitter(pkg: PackageInfo) -> Box<dyn Emitter<Kind>> {
    Box::new(GoEmitter { pkg })
}

struct GoEmitter {
    pkg: PackageInfo,
}

impl Emitter<Kind> for GoEmitter {
    fn language(&self) -> Language {
        Language::Go
    }

    fn emit(&self, ir: &IR<Kind>) -> Result<Vec<EmittedFile>> {
        let mut out = String::new();
        let has_service = !services(ir).is_empty();

        writeln!(out, "// {BANNER}").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "package {}", self.pkg.name).unwrap();
        writeln!(out).unwrap();
        render_imports(&mut out, has_service);
        out.push_str(SHARED_CORE);

        for svc in services(ir) {
            render_service(&mut out, ir, svc);
        }

        let aliases = collect_const_aliases(ir);
        if !aliases.is_empty() {
            section(&mut out, "const discriminators");
            for (alias, value_const, base, value) in &aliases {
                writeln!(
                    out,
                    "// {alias} is the open type backing a const discriminator (P13.1).\n\
                     type {alias} = {base}\n\n\
                     // {value_const} is the value asserted for the discriminator.\n\
                     const {value_const} = {alias}({value:?})\n"
                )
                .unwrap();
            }
        }

        for symbol in type_symbols(ir) {
            let name =
                naming::type_ident(symbol.name.as_str(), Language::Go, &NameOverrides::default());
            if let Some(record) = as_record(symbol) {
                render_record(&mut out, ir, &name, record);
            } else if let Some(map) = as_map(symbol) {
                render_map(&mut out, &name, map);
            }
        }

        Ok(vec![EmittedFile {
            path: PathBuf::from(format!("{}.go", self.pkg.name)),
            body: out,
        }])
    }
}

fn render_imports(out: &mut String, has_service: bool) {
    out.push_str("import (\n");
    out.push_str(
        "\t\"bytes\"\n\t\"encoding/json\"\n\t\"errors\"\n\t\"fmt\"\n\t\"math\"\n\t\"strings\"\n",
    );
    if has_service {
        out.push_str("\n\t\"github.com/nexus-rpc/sdk-go/nexus\"\n");
    }
    out.push_str(")\n");
}

fn section(out: &mut String, title: &str) {
    let bar = "// ---------------------------------------------------------------------------";
    writeln!(out, "\n{bar}\n// {title}\n{bar}\n").unwrap();
}

// ---------------------------------------------------------------------------
// Service binding.
// ---------------------------------------------------------------------------

fn render_service(out: &mut String, ir: &IR<Kind>, svc: &Service) {
    section(out, "Service binding.");
    let ident = naming::service_ident(svc.name.as_str(), Language::Go);
    if let Some(docs) = &svc.docs {
        writeln!(out, "// {ident} - {}", docs.trim()).unwrap();
    }
    writeln!(out, "var {ident} = struct {{").unwrap();
    writeln!(out, "\tServiceName string").unwrap();
    for op in &svc.operations {
        let op_ident = op.name.as_str().to_pascal_case();
        if let Some(docs) = &op.docs {
            writeln!(out, "\t// {op_ident} - {}", docs.trim()).unwrap();
        }
        let in_ty = io_type(ir, op.input);
        let out_ty = io_type(ir, op.output);
        writeln!(out, "\t{op_ident} nexus.OperationReference[{in_ty}, {out_ty}]").unwrap();
    }
    writeln!(out, "}}{{").unwrap();
    writeln!(out, "\tServiceName: {:?},", svc.wire_name).unwrap();
    for op in &svc.operations {
        let op_ident = op.name.as_str().to_pascal_case();
        let in_ty = io_type(ir, op.input);
        let out_ty = io_type(ir, op.output);
        writeln!(
            out,
            "\t{op_ident}: nexus.NewOperationReference[{in_ty}, {out_ty}]({:?}),",
            op.wire_name
        )
        .unwrap();
    }
    writeln!(out, "}}\n").unwrap();
}

fn io_type(ir: &IR<Kind>, id: Option<SymbolId>) -> String {
    match id {
        Some(id) => {
            naming::type_ident(symbol_name(ir, id), Language::Go, &NameOverrides::default())
        }
        None => "nexus.NoValue".to_string(),
    }
}

// ---------------------------------------------------------------------------
// const aliases.
// ---------------------------------------------------------------------------

/// (alias type name, value const name, base go type, value string).
fn collect_const_aliases(ir: &IR<Kind>) -> Vec<(String, String, String, String)> {
    let mut aliases = Vec::new();
    for symbol in type_symbols(ir) {
        let type_name =
            naming::type_ident(symbol.name.as_str(), Language::Go, &NameOverrides::default());
        if let Some(record) = as_record(symbol) {
            for field in &record.fields {
                if let Some(Scalar::String(value)) = &field.constant {
                    let alias = const_alias(&type_name, field);
                    let value_const = format!("{alias}{}", value.to_pascal_case());
                    aliases.push((alias, value_const, "string".to_string(), value.clone()));
                }
            }
        }
    }
    aliases
}

fn const_alias(type_name: &str, field: &Field) -> String {
    format!(
        "{type_name}{}",
        naming::field_ident(&field.json_name, Language::Go, &field.overrides)
    )
}

// ---------------------------------------------------------------------------
// Records.
// ---------------------------------------------------------------------------

fn render_record(out: &mut String, ir: &IR<Kind>, name: &str, record: &Record) {
    section(out, name);
    if let Some(docs) = &record.docs {
        for line in docs.trim().lines() {
            writeln!(out, "// {}", line.trim()).unwrap();
        }
    }

    writeln!(out, "type {name} struct {{").unwrap();
    for field in &record.fields {
        let go_name = naming::field_ident(&field.json_name, Language::Go, &field.overrides);
        let ty = go_field_type(ir, name, field);
        let tag = json_tag(field);
        if let Some(docs) = &field.docs {
            writeln!(out, "\t// {go_name} - {}", docs.trim().replace('\n', " ")).unwrap();
        }
        writeln!(out, "\t{go_name} {ty} `{tag}`").unwrap();
    }
    if record.additional == Additional::Open {
        writeln!(
            out,
            "\t// AdditionalProperties holds unknown members verbatim (forward compat, P13)."
        )
        .unwrap();
        writeln!(
            out,
            "\tAdditionalProperties map[string]json.RawMessage `json:\"-\"`"
        )
        .unwrap();
    }
    writeln!(out, "}}\n").unwrap();

    for field in &record.fields {
        if let Some(default) = &field.default {
            render_or_default(out, ir, name, field, default);
        }
    }

    render_validate(out, ir, name, record);
    render_unmarshal(out, ir, name, record);
    render_marshal(out, name, record);
}

fn render_or_default(out: &mut String, ir: &IR<Kind>, name: &str, field: &Field, default: &Scalar) {
    let go_name = naming::field_ident(&field.json_name, Language::Go, &field.overrides);
    let base = go_base(ir, &field.ty);
    let lit = scalar_lit(default);
    writeln!(
        out,
        "// {go_name}OrDefault returns {go_name} when set, else the schema default.\n\
         func (m {name}) {go_name}OrDefault() {base} {{\n\
         \tif m.{go_name} != nil {{\n\t\treturn *m.{go_name}\n\t}}\n\treturn {lit}\n}}\n"
    )
    .unwrap();
}

fn render_validate(out: &mut String, ir: &IR<Kind>, name: &str, record: &Record) {
    writeln!(out, "func (m {name}) Validate() error {{").unwrap();
    writeln!(out, "\tvar errs []Violation").unwrap();
    let _ = ir;
    for field in &record.fields {
        let go_name = naming::field_ident(&field.json_name, Language::Go, &field.overrides);
        let json = &field.json_name;
        if let Some(Scalar::String(value)) = &field.constant {
            let value_const = format!("{}{}", const_alias(name, field), value.to_pascal_case());
            writeln!(
                out,
                "\tif m.{go_name} != {value_const} {{\n\t\terrs = append(errs, Violation{{{json:?}, `const: must equal {value:?}`}})\n\t}}"
            )
            .unwrap();
        }
        match &field.ty {
            FieldType::Integer if is_pointer(field) => {
                writeln!(
                    out,
                    "\tif m.{go_name} != nil && (*m.{go_name} < -IntegerCap || *m.{go_name} > IntegerCap) {{\n\t\terrs = append(errs, Violation{{{json:?}, \"exceeds ±(2^53-1) integer cap\"}})\n\t}}"
                )
                .unwrap();
            }
            FieldType::Integer => {
                writeln!(
                    out,
                    "\tif m.{go_name} < -IntegerCap || m.{go_name} > IntegerCap {{\n\t\terrs = append(errs, Violation{{{json:?}, \"exceeds ±(2^53-1) integer cap\"}})\n\t}}"
                )
                .unwrap();
            }
            FieldType::Ref(_) if is_pointer(field) => {
                writeln!(
                    out,
                    "\tif m.{go_name} != nil {{\n\t\tmergeNested(&errs, {json:?}, m.{go_name}.Validate())\n\t}}"
                )
                .unwrap();
            }
            FieldType::Ref(_) => {
                writeln!(out, "\tmergeNested(&errs, {json:?}, m.{go_name}.Validate())").unwrap();
            }
            _ => {}
        }
    }
    if let Some(max) = record.max_properties {
        writeln!(
            out,
            "\tif len(m.AdditionalProperties) > {max} {{\n\t\terrs = append(errs, Violation{{\"\", fmt.Sprintf(\"maxProperties: at most {max} (got %d)\", len(m.AdditionalProperties))}})\n\t}}"
        )
        .unwrap();
    }
    writeln!(
        out,
        "\tif len(errs) > 0 {{\n\t\treturn &ValidationError{{Violations: errs}}\n\t}}\n\treturn nil\n}}\n"
    )
    .unwrap();
}

fn render_unmarshal(out: &mut String, ir: &IR<Kind>, name: &str, record: &Record) {
    writeln!(out, "func (m *{name}) UnmarshalJSON(data []byte) error {{").unwrap();
    writeln!(out, "\tvar all map[string]json.RawMessage").unwrap();
    writeln!(
        out,
        "\tif err := json.Unmarshal(data, &all); err != nil {{\n\t\treturn err\n\t}}"
    )
    .unwrap();
    writeln!(out, "\tvar errs []Violation").unwrap();

    let declared: Vec<String> = record
        .fields
        .iter()
        .map(|f| format!("{:?}", f.json_name))
        .collect();
    match record.additional {
        Additional::Open => {
            writeln!(
                out,
                "\tm.AdditionalProperties = map[string]json.RawMessage{{}}"
            )
            .unwrap();
            writeln!(out, "\tfor k, v := range all {{").unwrap();
            if declared.is_empty() {
                writeln!(out, "\t\tm.AdditionalProperties[k] = v").unwrap();
            } else {
                writeln!(
                    out,
                    "\t\tswitch k {{\n\t\tcase {}:\n\t\tdefault:\n\t\t\tm.AdditionalProperties[k] = v\n\t\t}}",
                    declared.join(", ")
                )
                .unwrap();
            }
            writeln!(out, "\t}}").unwrap();
        }
        Additional::Closed => {
            writeln!(out, "\tfor k := range all {{").unwrap();
            if declared.is_empty() {
                writeln!(
                    out,
                    "\t\terrs = append(errs, Violation{{k, \"unknown field\"}})"
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "\t\tswitch k {{\n\t\tcase {}:\n\t\tdefault:\n\t\t\terrs = append(errs, Violation{{k, \"unknown field\"}})\n\t\t}}",
                    declared.join(", ")
                )
                .unwrap();
            }
            writeln!(out, "\t}}").unwrap();
        }
        Additional::Typed(_) => {}
    }

    writeln!(
        out,
        "\tget := func(k string) *json.RawMessage {{\n\t\tif v, ok := all[k]; ok {{\n\t\t\treturn &v\n\t\t}}\n\t\treturn nil\n\t}}"
    )
    .unwrap();
    writeln!(out, "\t_ = get").unwrap();

    for field in &record.fields {
        render_field_unmarshal(out, ir, name, field);
    }

    writeln!(
        out,
        "\tif len(errs) > 0 {{\n\t\treturn &ValidationError{{Violations: errs}}\n\t}}\n\treturn nil\n}}\n"
    )
    .unwrap();
}

fn render_field_unmarshal(out: &mut String, ir: &IR<Kind>, type_name: &str, field: &Field) {
    let go_name = naming::field_ident(&field.json_name, Language::Go, &field.overrides);
    let json = &field.json_name;
    let req = field.required;
    let null = field.nullable;
    let assign = if is_pointer(field) {
        "&v".to_string()
    } else {
        "v".to_string()
    };

    match &field.ty {
        FieldType::String => {
            writeln!(
                out,
                "\tif v, ok := parseStringField(get({json:?}), {json:?}, {req}, {null}, &errs); ok {{\n\t\tm.{go_name} = {assign}"
            )
            .unwrap();
            if let Some(Scalar::String(value)) = &field.constant {
                let value_const =
                    format!("{}{}", const_alias(type_name, field), value.to_pascal_case());
                writeln!(
                    out,
                    "\t\tif v != {value_const} {{\n\t\t\terrs = append(errs, Violation{{{json:?}, `const: must equal {value:?}`}})\n\t\t}}"
                )
                .unwrap();
            }
            writeln!(out, "\t}}").unwrap();
        }
        FieldType::Integer => {
            writeln!(
                out,
                "\tif v, ok := parseIntegerField(get({json:?}), {json:?}, {req}, {null}, &errs); ok {{\n\t\tm.{go_name} = {assign}\n\t}}"
            )
            .unwrap();
        }
        FieldType::Ref(_) if req && !null => {
            writeln!(
                out,
                "\tif raw := get({json:?}); raw == nil {{\n\t\terrs = append(errs, Violation{{{json:?}, \"required\"}})\n\t}} else if isNull(*raw) {{\n\t\terrs = append(errs, Violation{{{json:?}, \"explicit null not allowed\"}})\n\t}} else {{\n\t\tmergeNested(&errs, {json:?}, json.Unmarshal(*raw, &m.{go_name}))\n\t}}"
            )
            .unwrap();
        }
        FieldType::Ref(_) => {
            let base = go_base(ir, &field.ty);
            writeln!(
                out,
                "\tif raw := get({json:?}); raw == nil {{\n\t}} else if isNull(*raw) {{\n\t\t{}\n\t}} else {{\n\t\tvar tmp {base}\n\t\tif err := json.Unmarshal(*raw, &tmp); err != nil {{\n\t\t\tmergeNested(&errs, {json:?}, err)\n\t\t}} else {{\n\t\t\tm.{go_name} = &tmp\n\t\t}}\n\t}}",
                null_branch(json, null)
            )
            .unwrap();
        }
        FieldType::Array(_) => {
            writeln!(
                out,
                "\tif raw := get({json:?}); raw == nil {{\n\t}} else if isNull(*raw) {{\n\t\t{}\n\t}} else if err := json.Unmarshal(*raw, &m.{go_name}); err != nil {{\n\t\terrs = append(errs, Violation{{{json:?}, \"expected array\"}})\n\t}}",
                null_branch(json, null)
            )
            .unwrap();
        }
        FieldType::Boolean | FieldType::Number => {
            let base = go_scalar_base(&field.ty);
            writeln!(
                out,
                "\tif raw := get({json:?}); raw == nil {{\n\t\tif {req} {{\n\t\t\terrs = append(errs, Violation{{{json:?}, \"required\"}})\n\t\t}}\n\t}} else if isNull(*raw) {{\n\t\t{}\n\t}} else {{\n\t\tvar v {base}\n\t\tif err := json.Unmarshal(*raw, &v); err != nil {{\n\t\t\terrs = append(errs, Violation{{{json:?}, \"expected value\"}})\n\t\t}} else {{\n\t\t\tm.{go_name} = {assign}\n\t\t}}\n\t}}",
                null_branch(json, null)
            )
            .unwrap();
        }
    }
}

fn null_branch(json: &str, nullable: bool) -> String {
    if nullable {
        String::new()
    } else {
        format!("errs = append(errs, Violation{{{json:?}, \"explicit null not allowed\"}})")
    }
}

fn render_marshal(out: &mut String, name: &str, record: &Record) {
    writeln!(out, "func (m {name}) MarshalJSON() ([]byte, error) {{").unwrap();
    writeln!(out, "\tvar errs []Violation").unwrap();
    writeln!(out, "\taddViolations(&errs, m.Validate())").unwrap();
    writeln!(out, "\tout := map[string]json.RawMessage{{}}").unwrap();
    if record.additional == Additional::Open {
        writeln!(
            out,
            "\tfor k, v := range m.AdditionalProperties {{\n\t\tout[k] = v\n\t}}"
        )
        .unwrap();
    }
    for field in &record.fields {
        let go_name = naming::field_ident(&field.json_name, Language::Go, &field.overrides);
        let json = &field.json_name;
        if field.required && field.nullable {
            writeln!(
                out,
                "\tif m.{go_name} != nil {{\n\t\tmarshalField(out, {json:?}, *m.{go_name}, &errs)\n\t}} else {{\n\t\tout[{json:?}] = json.RawMessage(\"null\")\n\t}}"
            )
            .unwrap();
        } else if is_pointer(field) {
            writeln!(
                out,
                "\tif m.{go_name} != nil {{\n\t\tmarshalField(out, {json:?}, *m.{go_name}, &errs)\n\t}}"
            )
            .unwrap();
        } else if matches!(field.ty, FieldType::Array(_)) && !field.required {
            writeln!(
                out,
                "\tif m.{go_name} != nil {{\n\t\tmarshalField(out, {json:?}, m.{go_name}, &errs)\n\t}}"
            )
            .unwrap();
        } else {
            writeln!(out, "\tmarshalField(out, {json:?}, m.{go_name}, &errs)").unwrap();
        }
    }
    writeln!(
        out,
        "\tif len(errs) > 0 {{\n\t\treturn nil, &ValidationError{{Violations: errs}}\n\t}}\n\treturn json.Marshal(out)\n}}\n"
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Maps.
// ---------------------------------------------------------------------------

fn render_map(out: &mut String, name: &str, map: &MapType) {
    section(out, name);
    if let Some(docs) = &map.docs {
        for line in docs.trim().lines() {
            writeln!(out, "// {}", line.trim()).unwrap();
        }
    }
    let val = go_scalar_base(&map.value);
    writeln!(
        out,
        "type {name} struct {{\n\tAdditionalProperties map[string]{val}\n}}\n"
    )
    .unwrap();

    writeln!(out, "func (m {name}) Validate() error {{\n\tvar errs []Violation").unwrap();
    if let Some(max) = map.max_properties {
        writeln!(
            out,
            "\tif len(m.AdditionalProperties) > {max} {{\n\t\terrs = append(errs, Violation{{\"\", fmt.Sprintf(\"maxProperties: at most {max} (got %d)\", len(m.AdditionalProperties))}})\n\t}}"
        )
        .unwrap();
    }
    if let Some(min) = map.min_properties {
        writeln!(
            out,
            "\tif len(m.AdditionalProperties) < {min} {{\n\t\terrs = append(errs, Violation{{\"\", fmt.Sprintf(\"minProperties: at least {min} (got %d)\", len(m.AdditionalProperties))}})\n\t}}"
        )
        .unwrap();
    }
    writeln!(
        out,
        "\tif len(errs) > 0 {{\n\t\treturn &ValidationError{{Violations: errs}}\n\t}}\n\treturn nil\n}}\n"
    )
    .unwrap();

    writeln!(out, "func (m *{name}) UnmarshalJSON(data []byte) error {{").unwrap();
    writeln!(out, "\tvar raw map[string]json.RawMessage").unwrap();
    writeln!(
        out,
        "\tif err := json.Unmarshal(data, &raw); err != nil {{\n\t\treturn err\n\t}}"
    )
    .unwrap();
    writeln!(out, "\tvar errs []Violation").unwrap();
    writeln!(
        out,
        "\tm.AdditionalProperties = make(map[string]{val}, len(raw))"
    )
    .unwrap();
    writeln!(out, "\tfor k, v := range raw {{").unwrap();
    writeln!(
        out,
        "\t\tif isNull(v) {{\n\t\t\terrs = append(errs, Violation{{k, \"explicit null not allowed\"}})\n\t\t\tcontinue\n\t\t}}"
    )
    .unwrap();
    writeln!(
        out,
        "\t\tvar s {val}\n\t\tif err := json.Unmarshal(v, &s); err != nil {{\n\t\t\terrs = append(errs, Violation{{k, \"expected {val}\"}})\n\t\t\tcontinue\n\t\t}}\n\t\tm.AdditionalProperties[k] = s\n\t}}"
    )
    .unwrap();
    if let Some(max) = map.max_properties {
        writeln!(
            out,
            "\tif len(m.AdditionalProperties) > {max} {{\n\t\terrs = append(errs, Violation{{\"\", fmt.Sprintf(\"maxProperties: at most {max} (got %d)\", len(m.AdditionalProperties))}})\n\t}}"
        )
        .unwrap();
    }
    writeln!(
        out,
        "\tif len(errs) > 0 {{\n\t\treturn &ValidationError{{Violations: errs}}\n\t}}\n\treturn nil\n}}\n"
    )
    .unwrap();

    writeln!(out, "func (m {name}) MarshalJSON() ([]byte, error) {{").unwrap();
    writeln!(
        out,
        "\tif err := m.Validate(); err != nil {{\n\t\treturn nil, err\n\t}}"
    )
    .unwrap();
    writeln!(
        out,
        "\tout := make(map[string]{val}, len(m.AdditionalProperties))\n\tfor k, v := range m.AdditionalProperties {{\n\t\tout[k] = v\n\t}}\n\treturn json.Marshal(out)\n}}\n"
    )
    .unwrap();
}

// ---------------------------------------------------------------------------
// Type mapping helpers.
// ---------------------------------------------------------------------------

fn is_pointer(field: &Field) -> bool {
    (field.nullable || !field.required) && !matches!(field.ty, FieldType::Array(_))
}

fn go_field_type(ir: &IR<Kind>, type_name: &str, field: &Field) -> String {
    if let Some(Scalar::String(_)) = &field.constant {
        if matches!(field.ty, FieldType::String) {
            return const_alias(type_name, field);
        }
    }
    let base = go_base(ir, &field.ty);
    if is_pointer(field) {
        format!("*{base}")
    } else {
        base
    }
}

fn go_base(ir: &IR<Kind>, ty: &FieldType) -> String {
    match ty {
        FieldType::String => "string".to_string(),
        FieldType::Integer => "int64".to_string(),
        FieldType::Number => "float64".to_string(),
        FieldType::Boolean => "bool".to_string(),
        FieldType::Array(inner) => format!("[]{}", go_base(ir, inner)),
        FieldType::Ref(id) => {
            naming::type_ident(symbol_name(ir, *id), Language::Go, &NameOverrides::default())
        }
    }
}

fn go_scalar_base(ty: &FieldType) -> String {
    match ty {
        FieldType::String => "string".to_string(),
        FieldType::Integer => "int64".to_string(),
        FieldType::Number => "float64".to_string(),
        FieldType::Boolean => "bool".to_string(),
        FieldType::Array(inner) => format!("[]{}", go_scalar_base(inner)),
        FieldType::Ref(_) => "json.RawMessage".to_string(),
    }
}

fn json_tag(field: &Field) -> String {
    if field.required {
        format!("json:{:?}", field.json_name)
    } else {
        format!("json:{:?}", format!("{},omitempty", field.json_name))
    }
}

fn scalar_lit(scalar: &Scalar) -> String {
    match scalar {
        Scalar::String(s) => format!("{s:?}"),
        Scalar::Integer(i) => i.to_string(),
        Scalar::Number(n) => n.to_string(),
        Scalar::Boolean(b) => b.to_string(),
    }
}

/// The shared validator core, emitted verbatim once per package.
const SHARED_CORE: &str = r#"
// ---------------------------------------------------------------------------
// Shared validator core (emitted once per package).
// ---------------------------------------------------------------------------

// Violation is a single constraint failure. Path is the JSON member path
// (dotted for nested members); Reason is a human-readable message.
type Violation struct {
	Path   string
	Reason string
}

func (v Violation) String() string {
	if v.Path == "" {
		return v.Reason
	}
	return v.Path + ": " + v.Reason
}

// ValidationError aggregates every Violation found while (de)serializing a
// value, surfacing them all in one error (never a partial first-failure).
type ValidationError struct {
	Violations []Violation
}

func (e *ValidationError) Error() string {
	parts := make([]string, len(e.Violations))
	for i, v := range e.Violations {
		parts[i] = v.String()
	}
	return fmt.Sprintf("%d validation error(s): %s", len(e.Violations), strings.Join(parts, "; "))
}

// addViolations folds a child error's violations into errs at the same path
// scope (no prefix).
func addViolations(errs *[]Violation, err error) {
	if err == nil {
		return
	}
	var ve *ValidationError
	if errors.As(err, &ve) {
		*errs = append(*errs, ve.Violations...)
		return
	}
	*errs = append(*errs, Violation{"", err.Error()})
}

// mergeNested appends a child value's violations under a dotted parent path.
func mergeNested(errs *[]Violation, path string, err error) {
	if err == nil {
		return
	}
	var ve *ValidationError
	if errors.As(err, &ve) {
		for _, v := range ve.Violations {
			p := v.Path
			if p == "" {
				p = path
			} else {
				p = path + "." + v.Path
			}
			*errs = append(*errs, Violation{p, v.Reason})
		}
		return
	}
	*errs = append(*errs, Violation{path, err.Error()})
}

// IntegerCap is the cross-language integer bound, ±(2^53-1), equal to
// JavaScript's Number.MAX_SAFE_INTEGER.
const IntegerCap = 1<<53 - 1

var (
	errFractional = errors.New("not an integer")
	errRange      = errors.New("exceeds ±(2^53-1) integer cap")
)

// parseSpecInteger accepts spec-valid integers ("1", "1.0", "1e2"), rejects
// fractional values ("1.5"), and enforces the ±(2^53-1) cap.
func parseSpecInteger(n json.Number) (int64, error) {
	f, err := n.Float64()
	if err != nil {
		return 0, err
	}
	if f != math.Trunc(f) {
		return 0, errFractional
	}
	if f < -IntegerCap || f > IntegerCap {
		return 0, errRange
	}
	i, err := n.Int64()
	if err != nil {
		return 0, err
	}
	return i, nil
}

// isNull reports whether a captured raw member is the JSON null literal.
func isNull(raw json.RawMessage) bool {
	return bytes.Equal(bytes.TrimSpace(raw), []byte("null"))
}

// parseStringField applies the absent/null/value three-way for a string member.
func parseStringField(raw *json.RawMessage, path string, required, nullable bool, errs *[]Violation) (string, bool) {
	if raw == nil {
		if required {
			*errs = append(*errs, Violation{path, "required"})
		}
		return "", false
	}
	if isNull(*raw) {
		if !nullable {
			*errs = append(*errs, Violation{path, "explicit null not allowed"})
		}
		return "", false
	}
	var s string
	if err := json.Unmarshal(*raw, &s); err != nil {
		*errs = append(*errs, Violation{path, "expected string"})
		return "", false
	}
	return s, true
}

// parseIntegerField applies the three-way and the spec-integer parse.
func parseIntegerField(raw *json.RawMessage, path string, required, nullable bool, errs *[]Violation) (int64, bool) {
	if raw == nil {
		if required {
			*errs = append(*errs, Violation{path, "required"})
		}
		return 0, false
	}
	if isNull(*raw) {
		if !nullable {
			*errs = append(*errs, Violation{path, "explicit null not allowed"})
		}
		return 0, false
	}
	dec := json.NewDecoder(bytes.NewReader(*raw))
	dec.UseNumber()
	var n json.Number
	if err := dec.Decode(&n); err != nil {
		*errs = append(*errs, Violation{path, "expected integer"})
		return 0, false
	}
	v, err := parseSpecInteger(n)
	if err != nil {
		*errs = append(*errs, Violation{path, err.Error()})
		return 0, false
	}
	return v, true
}

// marshalField encodes v and stores it under key, collecting any violations
// under key's path.
func marshalField(out map[string]json.RawMessage, key string, v any, errs *[]Violation) {
	b, err := json.Marshal(v)
	if err != nil {
		mergeNested(errs, key, err)
		return
	}
	out[key] = b
}
"#;
