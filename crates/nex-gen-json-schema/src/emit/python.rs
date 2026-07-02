//! The Python emitter.
//!
//! Renders one self-contained module (`<pkg>/__init__.py`) for the single-input
//! layout: the shared validator core, the `@nexusrpc.service` binding, and one
//! Pydantic `BaseModel` per type. Models carry a `@model_serializer` that emits
//! only the wire keys actually set (omit-unset for `default`, faithful
//! optional+nullable round-trip) so they work through the default Temporal
//! `pydantic_data_converter`.

use std::fmt::Write as _;
use std::path::PathBuf;

use heck::ToPascalCase;
use nex_gen_core::{Emitter, EmittedFile, IR, Language, Result, Service, SymbolId};

use crate::emit::{PackageInfo, as_map, as_record, doc_lines, services, symbol_name, type_symbols};
use crate::ir::{Additional, Field, FieldType, Kind, MapType, Record, Scalar};
use crate::naming;

const LANG: Language = Language::Python;

/// Build the Python emitter.
pub fn emitter(pkg: PackageInfo) -> Box<dyn Emitter<Kind>> {
    Box::new(PythonEmitter { pkg })
}

struct PythonEmitter {
    pkg: PackageInfo,
}

impl Emitter<Kind> for PythonEmitter {
    fn language(&self) -> Language {
        LANG
    }

    fn emit(&self, ir: &IR<Kind>) -> Result<Vec<EmittedFile>> {
        let mut out = String::new();

        // Header + imports + __all__.
        write_header(&mut out, &self.pkg.name, ir);
        out.push_str(SHARED_CORE);

        // Models are emitted before the service: `nexusrpc.service` eagerly
        // evaluates the `Operation[In, Out]` annotations at decoration time, so
        // the I/O model classes must already exist.
        out.push_str(MODELS_BANNER);
        for symbol in type_symbols(ir) {
            if let Some(record) = as_record(symbol) {
                render_record(&mut out, symbol.name.as_str(), record, ir);
            } else if let Some(map) = as_map(symbol) {
                render_map(&mut out, symbol.name.as_str(), map);
            }
        }

        // model_rebuild() for every model (resolves forward refs / cycles).
        out.push_str("\n# Resolve forward references (const ClassVars, $ref cycles).\n");
        for symbol in type_symbols(ir) {
            let _ = writeln!(out, "{}.model_rebuild()", type_name(symbol.name.as_str()));
        }

        // Service binding (after models, whose classes it references).
        for svc in services(ir) {
            render_service(&mut out, svc, ir);
        }

        let path = PathBuf::from(format!("{}/__init__.py", self.pkg.name));
        Ok(vec![EmittedFile { path, body: out }])
    }
}

// ---------------------------------------------------------------------------
// Header.
// ---------------------------------------------------------------------------

fn write_header(out: &mut String, module: &str, ir: &IR<Kind>) {
    let _ = writeln!(out, "# {}", crate::emit::BANNER);
    let _ = writeln!(out, "#");
    let _ = writeln!(out, "# Source: {module}.nexusrpc.yaml");
    out.push_str(HEADER_IMPORTS);

    // __all__: ValidationError, services, then model type names in id order.
    out.push_str("__all__ = [\n    \"ValidationError\",\n");
    for svc in services(ir) {
        let _ = writeln!(out, "    \"{}\",", type_name(svc.name.as_str()));
    }
    for symbol in type_symbols(ir) {
        let _ = writeln!(out, "    \"{}\",", type_name(symbol.name.as_str()));
    }
    out.push_str("]\n");
}

// ---------------------------------------------------------------------------
// Service binding.
// ---------------------------------------------------------------------------

fn render_service(out: &mut String, svc: &Service, ir: &IR<Kind>) {
    out.push_str(SERVICE_BANNER);
    let _ = writeln!(out, "@nexusrpc.service(name=\"{}\")", svc.wire_name);
    let _ = writeln!(out, "class {}:", type_name(svc.name.as_str()));
    if let Some(docs) = &svc.docs {
        let _ = writeln!(out, "    \"\"\"{}\"\"\"", first_line(docs));
    }
    out.push('\n');
    for op in &svc.operations {
        let attr = naming::map_case(op.name.as_str(), LANG);
        let input = io_type(op.input, ir);
        let output = io_type(op.output, ir);
        let _ = writeln!(
            out,
            "    {attr}: Operation[{input}, {output}] = Operation(name=\"{}\")",
            op.wire_name
        );
        if let Some(docs) = &op.docs {
            let _ = writeln!(out, "    \"\"\"{}\"\"\"", first_line(docs));
        }
        out.push('\n');
    }
}

fn io_type(id: Option<SymbolId>, ir: &IR<Kind>) -> String {
    match id {
        Some(id) => type_name(symbol_name(ir, id)),
        None => "None".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Records.
// ---------------------------------------------------------------------------

fn render_record(out: &mut String, name: &str, record: &Record, ir: &IR<Kind>) {
    let class = type_name(name);
    let _ = writeln!(out, "\nclass {class}(BaseModel):");
    if let Some(docs) = &record.docs {
        let _ = writeln!(out, "    \"\"\"{}\"\"\"", first_line(docs));
    } else if record.synthesized {
        let _ = writeln!(out, "    # Synthesized from an inline operation object.");
    }
    out.push('\n');

    let extra = match record.additional {
        Additional::Closed => "forbid",
        Additional::Open | Additional::Typed(_) => "allow",
    };
    let _ = writeln!(
        out,
        "    model_config = ConfigDict(strict=True, populate_by_name=True, extra=\"{extra}\")",
    );
    out.push('\n');

    // const ClassVar type aliases.
    let const_fields: Vec<&Field> =
        record.fields.iter().filter(|f| f.constant.is_some()).collect();
    for field in &const_fields {
        let pascal = field.json_name.to_pascal_case();
        if let Some(Scalar::String(v)) = &field.constant {
            let _ = writeln!(out, "    # Open-enum hint for the `const` discriminator (P13.1); the");
            let _ = writeln!(out, "    # validator closes it to \"{v}\".");
            let _ = writeln!(out, "    {pascal}: ClassVar = Union[Literal[\"{v}\"], str]");
            out.push('\n');
        }
    }

    // Fields.
    for field in &record.fields {
        render_field(out, &class, field, ir);
    }

    // const / optional-non-nullable ClassVars.
    for field in &const_fields {
        if let Some(Scalar::String(v)) = &field.constant {
            let upper = field.json_name.to_uppercase();
            let _ = writeln!(out, "    _{upper}_CONST: ClassVar[str] = \"{v}\"");
        }
    }
    let opt_non_null: Vec<&str> = record
        .fields
        .iter()
        .filter(|f| !f.required && !f.nullable && f.constant.is_none())
        .map(|f| f.json_name.as_str())
        .collect();
    if !opt_non_null.is_empty() {
        let set = opt_non_null
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "    _OPTIONAL_NON_NULLABLE: ClassVar[frozenset] = frozenset({{{set}}})"
        );
    }
    if !const_fields.is_empty() || !opt_non_null.is_empty() {
        out.push('\n');
    }

    // Validators.
    for field in &const_fields {
        if let Some(Scalar::String(_)) = &field.constant {
            render_const_validator(out, field);
        }
    }
    if !opt_non_null.is_empty() {
        out.push_str(REJECT_NULL_VALIDATOR);
    }

    // Serializer.
    out.push_str(SERIALIZER);
}

fn render_field(out: &mut String, class: &str, field: &Field, ir: &IR<Kind>) {
    let py_name = naming::field_ident(&field.json_name, LANG, &field.overrides);
    let annotation = annotation(class, field, ir);
    let field_call = field_call(&py_name, field);
    let _ = writeln!(out, "    {py_name}: {annotation} = {field_call}");
    if let Some(docs) = &field.docs {
        let _ = writeln!(out, "    \"\"\"{}\"\"\"", first_line(docs));
    }
    out.push('\n');
}

/// The type annotation for a field.
fn annotation(class: &str, field: &Field, ir: &IR<Kind>) -> String {
    if field.constant.is_some() {
        // Reference the ClassVar open-enum type: "Class.Field".
        return format!("\"{class}.{}\"", field.json_name.to_pascal_case());
    }
    let base = base_type(&field.ty, ir);
    if field.nullable || !field.required {
        format!("Optional[{base}]")
    } else {
        base
    }
}

/// The Pydantic base type for a field type.
fn base_type(ty: &FieldType, ir: &IR<Kind>) -> String {
    match ty {
        FieldType::String => "str".to_string(),
        FieldType::Integer => "SpecInt".to_string(),
        FieldType::Number => "float".to_string(),
        FieldType::Boolean => "bool".to_string(),
        FieldType::Array(inner) => format!("list[{}]", base_type(inner, ir)),
        FieldType::Ref(id) => type_name(symbol_name(ir, *id)),
    }
}

/// The `Field(...)` construction call.
fn field_call(py_name: &str, field: &Field) -> String {
    let mut args: Vec<String> = Vec::new();
    if let Some(default) = &field.default {
        args.push(format!("default={}", scalar_literal(default)));
    } else if !field.required && field.constant.is_none() {
        args.push("default=None".to_string());
    }
    if py_name != field.json_name {
        args.push(format!("alias=\"{}\"", field.json_name));
    }
    format!("Field({})", args.join(", "))
}

fn render_const_validator(out: &mut String, field: &Field) {
    let py_name = naming::field_ident(&field.json_name, LANG, &field.overrides);
    let json = &field.json_name;
    let upper = json.to_uppercase();
    let _ = writeln!(out, "    @model_validator(mode=\"before\")");
    let _ = writeln!(out, "    @classmethod");
    let _ = writeln!(out, "    def _const_{py_name}(cls, data):");
    let _ = writeln!(out, "        if isinstance(data, dict):");
    let _ = writeln!(out, "            if \"{json}\" not in data:");
    let _ = writeln!(out, "                data = {{**data, \"{json}\": cls._{upper}_CONST}}");
    let _ = writeln!(out, "            elif data[\"{json}\"] != cls._{upper}_CONST:");
    let _ = writeln!(out, "                raise PydanticCustomError(");
    let _ = writeln!(
        out,
        "                    \"const\", f'{json} must equal \"{{cls._{upper}_CONST}}\"'"
    );
    let _ = writeln!(out, "                )");
    let _ = writeln!(out, "        return data");
    out.push('\n');
}

// ---------------------------------------------------------------------------
// Typed maps.
// ---------------------------------------------------------------------------

fn render_map(out: &mut String, name: &str, map: &MapType) {
    let class = type_name(name);
    let _ = writeln!(out, "\nclass {class}(BaseModel):");
    if let Some(docs) = &map.docs {
        let _ = writeln!(out, "    \"\"\"{}\"\"\"", first_line(docs));
    }
    out.push('\n');
    let _ = writeln!(out, "    # Typed map -> named wrapper: extras live in `model_extra`, each");
    let _ = writeln!(out, "    # validated by type; count capped at maxProperties.");
    let _ = writeln!(out, "    model_config = ConfigDict(strict=True, extra=\"allow\")");
    out.push('\n');
    if let Some(max) = map.max_properties {
        let _ = writeln!(out, "    _MAX_PROPERTIES: ClassVar[int] = {max}");
        out.push('\n');
    }

    let is_string = matches!(map.value, FieldType::String);
    let _ = writeln!(out, "    @model_validator(mode=\"after\")");
    let _ = writeln!(out, "    def _validate_extras(self):");
    let _ = writeln!(out, "        errs = []");
    let _ = writeln!(out, "        extra = self.model_extra or {{}}");
    if is_string {
        let _ = writeln!(out, "        for key, value in extra.items():");
        let _ = writeln!(out, "            if not isinstance(value, str):");
        let _ = writeln!(out, "                errs.append(");
        let _ = writeln!(out, "                    InitErrorDetails(");
        let _ = writeln!(out, "                        type=PydanticCustomError(");
        let _ = writeln!(out, "                            \"string_type\", \"expected string value\"");
        let _ = writeln!(out, "                        ),");
        let _ = writeln!(out, "                        loc=(key,),");
        let _ = writeln!(out, "                        input=value,");
        let _ = writeln!(out, "                    )");
        let _ = writeln!(out, "                )");
    }
    if map.max_properties.is_some() {
        let _ = writeln!(out, "        if len(extra) > self._MAX_PROPERTIES:");
        let _ = writeln!(out, "            errs.append(");
        let _ = writeln!(out, "                InitErrorDetails(");
        let _ = writeln!(out, "                    type=PydanticCustomError(");
        let _ = writeln!(out, "                        \"too_many_properties\",");
        let _ = writeln!(
            out,
            "                        f\"at most {{self._MAX_PROPERTIES}} properties allowed\","
        );
        let _ = writeln!(out, "                    ),");
        let _ = writeln!(out, "                    loc=(),");
        let _ = writeln!(out, "                    input=len(extra),");
        let _ = writeln!(out, "                )");
        let _ = writeln!(out, "            )");
    }
    let _ = writeln!(out, "        if errs:");
    let _ = writeln!(out, "            raise ValidationError.from_exception_data(");
    let _ = writeln!(out, "                title=type(self).__name__, line_errors=errs");
    let _ = writeln!(out, "            )");
    let _ = writeln!(out, "        return self");
    out.push('\n');
    let _ = writeln!(out, "    @model_serializer(mode=\"wrap\")");
    let _ = writeln!(out, "    def _serialize(self, handler):");
    let _ = writeln!(out, "        return dict(self.model_extra or {{}})");
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn type_name(name: &str) -> String {
    naming::type_ident(name, LANG, &crate::ir::NameOverrides::default())
}

fn first_line(docs: &str) -> String {
    doc_lines(docs).join(" ")
}

fn scalar_literal(scalar: &Scalar) -> String {
    match scalar {
        Scalar::String(s) => format!("\"{s}\""),
        Scalar::Integer(i) => i.to_string(),
        Scalar::Number(n) => n.to_string(),
        Scalar::Boolean(b) => if *b { "True" } else { "False" }.to_string(),
    }
}

const HEADER_IMPORTS: &str = r#"#
# Single-input layout: one self-contained module. Domain models, the
# Nexus service binding, and the shared validator boilerplate all live here.

from __future__ import annotations

from typing import Annotated, Any, ClassVar, Literal, Optional, Union

import nexusrpc
from nexusrpc import Operation
from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    ValidationError,
    model_serializer,
    model_validator,
)
from pydantic.functional_validators import BeforeValidator
from pydantic_core import InitErrorDetails, PydanticCustomError

"#;

const SHARED_CORE: &str = r#"
# ---------------------------------------------------------------------------
# Shared validator core (boilerplate, emitted once per package).
# ---------------------------------------------------------------------------

#: Cross-language integer cap: ±(2**53 - 1), i.e. Number.MAX_SAFE_INTEGER.
_INTEGER_CAP = (1 << 53) - 1


def _parse_spec_integer(v: Any) -> int:
    """Spec-compliant integer parse for ``type: integer``.

    Accepts ``int`` and ``float`` with a zero fractional part; rejects
    ``bool``, non-integral floats (``1.5``), and values past ±(2**53-1).
    """
    if isinstance(v, bool):
        raise ValueError("expected integer, got boolean")
    if isinstance(v, int):
        out = v
    elif isinstance(v, float):
        if v != int(v):
            raise ValueError("number has a fractional part; not an integer")
        out = int(v)
    else:
        raise ValueError(f"expected integer, got {type(v).__name__}")
    if abs(out) > _INTEGER_CAP:
        raise ValueError("integer exceeds ±(2**53-1) cap")
    return out


#: Field type for ``type: integer``. The user-facing type stays ``int``.
SpecInt = Annotated[int, BeforeValidator(_parse_spec_integer)]


def _reject_explicit_null(cls: type[BaseModel], data: Any, handler):
    """Reject an explicit wire ``null`` on an optional-non-nullable member."""
    pre_errs = []
    if isinstance(data, dict):
        pre_errs = [
            InitErrorDetails(
                type=PydanticCustomError(
                    "null_for_nonnullable", "explicit null not allowed"
                ),
                loc=(f,),
                input=None,
            )
            for f in cls._OPTIONAL_NON_NULLABLE
            if f in data and data[f] is None
        ]
    try:
        instance = handler(data)
    except ValidationError as e:
        field_errs = [
            InitErrorDetails(
                type=PydanticCustomError(err["type"], err["msg"]),
                loc=err["loc"],
                input=err.get("input"),
            )
            for err in e.errors()
        ]
        raise ValidationError.from_exception_data(
            title=cls.__name__, line_errors=pre_errs + field_errs
        ) from None
    if pre_errs:
        raise ValidationError.from_exception_data(
            title=cls.__name__, line_errors=pre_errs
        )
    return instance


def _emit_set_fields(model: BaseModel, handler) -> dict[str, Any]:
    """Serialize only the keys the wire actually carried (omit-unset)."""
    dumped = handler(model)
    alias_of = {
        name: (f.alias or name) for name, f in type(model).model_fields.items()
    }
    keep = {alias_of.get(n, n) for n in model.model_fields_set}
    out = {k: v for k, v in dumped.items() if k in keep}
    if model.model_extra:
        out.update(model.model_extra)
    return out

"#;

const SERVICE_BANNER: &str = r#"
# ---------------------------------------------------------------------------
# Service binding.
# ---------------------------------------------------------------------------


"#;

const MODELS_BANNER: &str = r#"
# ---------------------------------------------------------------------------
# Models.
# ---------------------------------------------------------------------------

"#;

const REJECT_NULL_VALIDATOR: &str = r#"    @model_validator(mode="wrap")
    @classmethod
    def _reject_null(cls, data, handler):
        return _reject_explicit_null(cls, data, handler)

"#;

const SERIALIZER: &str = r#"    @model_serializer(mode="wrap")
    def _serialize(self, handler):
        return _emit_set_fields(self, handler)
"#;
