use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use heck::{ToSnakeCase, ToUpperCamelCase};
use wit_parser::{
    Function, Interface, PackageId, Resolve, Type, TypeDef, TypeDefKind, WorldItem, WorldKey,
};

use crate::error::{Error, Result};
use crate::language::Language;

#[derive(Debug, Clone, PartialEq)]
pub struct ApiSpec {
    pub version: String,
    pub support: SupportSpec,
    pub services: Vec<ServiceSpec>,
    pub types: BTreeMap<String, TypeOverrideSpec>,
}

impl ApiSpec {
    pub fn load_for_language(language: Language, path: &Path) -> Result<Self> {
        let input = fs::read_to_string(path).map_err(|source| Error::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse_for_language(language, &input, path.to_path_buf())
    }

    pub fn parse_for_language(language: Language, input: &str, path: PathBuf) -> Result<Self> {
        let mut resolve = Resolve::default();
        let package_id = resolve
            .push_str(&path, input)
            .map_err(|error| Error::WitParse {
                path: path.clone(),
                message: error.to_string(),
            })?;
        Self::from_wit(language, &resolve, package_id, path)
    }

    pub fn type_override(&self, type_name: &str) -> Option<&TypeOverrideSpec> {
        self.types.get(type_name.trim_start_matches('.'))
    }

    fn from_wit(
        language: Language,
        resolve: &Resolve,
        package_id: PackageId,
        path: PathBuf,
    ) -> Result<Self> {
        let package = &resolve.packages[package_id];
        let world_id = select_world(resolve, package_id, &path)?;
        let world = &resolve.worlds[world_id];

        let support = SupportSpec {
            file: find_language_directive_value(
                language,
                &[
                    package.docs.contents.as_deref(),
                    world.docs.contents.as_deref(),
                ],
                &path,
                "package/world",
                "support",
            )?,
        };

        let mut types = BTreeMap::new();
        for interface_id in package.interfaces.values() {
            let interface = &resolve.interfaces[*interface_id];
            collect_interface_types(language, resolve, interface, &path, &mut types)?;
        }

        let mut services = Vec::new();
        for (key, item) in &world.exports {
            let WorldItem::Interface { id, .. } = item else {
                continue;
            };
            let interface = &resolve.interfaces[*id];
            services.push(build_service(language, resolve, key, interface, &path)?);
        }

        Ok(Self {
            version: package
                .name
                .version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "0.0.0".to_string()),
            support,
            services,
            types,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceSpec {
    pub name: String,
    pub endpoint: Option<String>,
    pub operations: Vec<OperationSpec>,
}

impl ServiceSpec {
    pub fn operation(&self, name: &str) -> Option<&OperationSpec> {
        self.operations
            .iter()
            .find(|operation| operation.name == name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportSpec {
    pub file: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationSpec {
    pub name: String,
    pub input_ref: String,
    pub output_ref: String,
    pub output_transform: Option<OperationOutputTransformSpec>,
}

impl OperationSpec {
    pub fn reference(&self, direction: Direction) -> &str {
        match direction {
            Direction::Input => &self.input_ref,
            Direction::Output => &self.output_ref,
        }
    }

    pub fn output_transform(&self) -> Option<&OperationOutputTransformSpec> {
        self.output_transform.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationOutputTransformSpec {
    pub type_name: String,
    pub transform: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeOverrideSpec {
    pub required_fields: BTreeSet<String>,
    pub omitted_fields: BTreeSet<String>,
    pub replacement: Option<TypeReplacementSpec>,
    pub generated_model: GeneratedModelSpec,
}

impl TypeOverrideSpec {
    pub fn is_field_required(&self, field_name: &str) -> bool {
        self.required_fields.contains(field_name)
    }

    pub fn is_field_omitted(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
    }

    pub fn is_field_hidden(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
            || self.field_source(field_name).is_some()
            || (!self.generated_model.declared_fields.is_empty()
                && !self.generated_model.declared_fields.contains(field_name))
    }

    pub fn replacement(&self) -> Option<&TypeReplacementSpec> {
        self.replacement.as_ref()
    }

    pub fn generated_model(&self) -> Option<&GeneratedModelSpec> {
        if self.generated_model.is_empty() {
            None
        } else {
            Some(&self.generated_model)
        }
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.generated_model()?.field_source(field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeReplacementSpec {
    pub type_name: String,
    pub from_proto: Option<String>,
    pub to_proto: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratedModelSpec {
    pub declared_fields: BTreeSet<String>,
    pub field_names: BTreeMap<String, String>,
    pub field_annotations: BTreeMap<String, String>,
    pub field_sources: BTreeMap<String, String>,
    pub functions: BTreeMap<String, FunctionFieldSpec>,
    pub with_arguments: BTreeMap<String, WithArgumentsFieldSpec>,
}

impl GeneratedModelSpec {
    pub fn is_empty(&self) -> bool {
        self.declared_fields.is_empty()
            && self.field_names.is_empty()
            && self.field_annotations.is_empty()
            && self.field_sources.is_empty()
            && self.functions.is_empty()
            && self.with_arguments.is_empty()
    }

    pub fn field_name_override(&self, field_name: &str) -> Option<&str> {
        self.field_names.get(field_name).map(String::as_str)
    }

    pub fn field_annotation(&self, field_name: &str) -> Option<&str> {
        self.field_annotations.get(field_name).map(String::as_str)
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.field_sources.get(field_name).map(String::as_str)
    }

    pub fn function(&self, field_name: &str) -> Option<&FunctionFieldSpec> {
        self.functions.get(field_name)
    }

    pub fn function_for_args_field(&self, field_name: &str) -> Option<&FunctionFieldSpec> {
        self.functions
            .values()
            .find(|function| function.args_field == field_name)
    }

    pub fn with_arguments(&self, field_name: &str) -> Option<&WithArgumentsFieldSpec> {
        self.with_arguments.get(field_name)
    }

    pub fn with_arguments_for_args_field(
        &self,
        field_name: &str,
    ) -> Option<&WithArgumentsFieldSpec> {
        self.with_arguments
            .values()
            .find(|with_arguments| with_arguments.args_field == field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFieldSpec {
    pub primary: bool,
    pub result_type: String,
    pub args_field: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithArgumentsFieldSpec {
    pub args_field: String,
    pub value_type: String,
    pub args_type: String,
    pub name_expr: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Direction {
    Input,
    Output,
}

fn collect_interface_types(
    language: Language,
    resolve: &Resolve,
    interface: &Interface,
    path: &Path,
    types: &mut BTreeMap<String, TypeOverrideSpec>,
) -> Result<()> {
    let interface_name = interface
        .name
        .as_deref()
        .unwrap_or("unnamed-interface")
        .to_string();
    for type_id in interface.types.values() {
        let type_def = &resolve.types[*type_id];
        let Some((proto_name, type_override)) =
            build_type_override(language, resolve, type_def, path, &interface_name)?
        else {
            continue;
        };
        if types.insert(proto_name.clone(), type_override).is_some() {
            return Err(Error::InvalidWit {
                path: path.to_path_buf(),
                reason: format!("duplicate `@nexus.proto` mapping for `{proto_name}`"),
            });
        }
    }

    Ok(())
}

fn build_type_override(
    language: Language,
    resolve: &Resolve,
    type_def: &TypeDef,
    path: &Path,
    interface_name: &str,
) -> Result<Option<(String, TypeOverrideSpec)>> {
    let type_name = type_def.name.as_deref().unwrap_or("unnamed-type");
    let context = format!("type `{interface_name}.{type_name}`");
    let directives = parse_directives(type_def.docs.contents.as_deref(), path, &context)?;
    let Some(proto_name) = directive_value(&directives, "proto", path, &context, "value")? else {
        return Ok(None);
    };

    let replacement = build_type_replacement(language, &directives, path, &context, &proto_name)?;

    let (required_fields, generated_model) = match &type_def.kind {
        TypeDefKind::Record(record) => {
            build_generated_model_from_record(language, resolve, record, path, &context)?
        }
        _ => (BTreeSet::new(), GeneratedModelSpec::default()),
    };

    let type_override = TypeOverrideSpec {
        required_fields,
        omitted_fields: BTreeSet::new(),
        replacement,
        generated_model,
    };

    Ok(Some((proto_name, type_override)))
}

fn build_generated_model_from_record(
    language: Language,
    resolve: &Resolve,
    record: &wit_parser::Record,
    path: &Path,
    context: &str,
) -> Result<(BTreeSet<String>, GeneratedModelSpec)> {
    let mut required_fields = BTreeSet::new();
    let mut declared_fields = BTreeSet::new();
    let mut field_names = BTreeMap::new();
    let mut field_annotations = BTreeMap::new();
    let mut field_sources = BTreeMap::new();
    let mut functions = BTreeMap::new();
    let mut with_arguments = BTreeMap::new();

    for field in &record.fields {
        let field_context = format!("{context} field `{}`", field.name);
        let directives = parse_directives(field.docs.contents.as_deref(), path, &field_context)?;
        let proto_field_name =
            directive_value(&directives, "proto-field", path, &field_context, "value")?
                .unwrap_or_else(|| field.name.to_snake_case());

        if !declared_fields.insert(proto_field_name.clone()) {
            return Err(Error::InvalidWit {
                path: path.to_path_buf(),
                reason: format!(
                    "{field_context} maps to duplicate proto field `{proto_field_name}`"
                ),
            });
        }

        field_names.insert(proto_field_name.clone(), field.name.clone());

        if !is_optional_type(resolve, &field.ty) {
            required_fields.insert(proto_field_name.clone());
        }

        if let Some(source) = find_language_directive_value(
            language,
            &[field.docs.contents.as_deref()],
            path,
            &field_context,
            "source",
        )? {
            field_sources.insert(proto_field_name.clone(), source);
        }

        if let Some(annotation) = find_language_directive_value(
            language,
            &[field.docs.contents.as_deref()],
            path,
            &field_context,
            "type",
        )? {
            field_annotations.insert(proto_field_name.clone(), annotation);
        }

        if let Some(function) = build_function_field(language, &directives, path, &field_context)? {
            functions.insert(proto_field_name.clone(), function);
        }

        if let Some(with_arguments_field) =
            build_with_arguments_field(language, &directives, path, &field_context)?
        {
            with_arguments.insert(proto_field_name.clone(), with_arguments_field);
        }

        if field_sources.contains_key(&proto_field_name)
            && functions.contains_key(&proto_field_name)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: context.to_string(),
                field: proto_field_name,
                property: "source",
                conflicting_property: "function",
            });
        }
    }

    Ok((
        required_fields,
        GeneratedModelSpec {
            declared_fields,
            field_names,
            field_annotations,
            field_sources,
            functions,
            with_arguments,
        },
    ))
}

fn build_type_replacement(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
    type_name: &str,
) -> Result<Option<TypeReplacementSpec>> {
    let Some(directive) = directive(directives, "type", path, context)? else {
        return Ok(None);
    };

    let selected_from_proto = directive
        .value(&format!("{}-from", language_key(language)))
        .map(ToOwned::to_owned);
    let selected_to_proto = directive
        .value(&format!("{}-to", language_key(language)))
        .map(ToOwned::to_owned);
    let selected_type_name =
        directive_language_value(directive, language).or_else(|| directive.value("value"));

    let Some(selected_type_name) = selected_type_name else {
        if selected_from_proto.is_some() || selected_to_proto.is_some() {
            return Err(Error::IncompleteTypeOverride {
                type_name: type_name.to_string(),
            });
        }
        return Ok(None);
    };

    Ok(Some(TypeReplacementSpec {
        type_name: selected_type_name.to_string(),
        from_proto: selected_from_proto,
        to_proto: selected_to_proto,
    }))
}

fn build_function_field(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
) -> Result<Option<FunctionFieldSpec>> {
    let Some(directive) = directive(directives, "function", path, context)? else {
        return Ok(None);
    };

    let Some(result_type) = directive
        .value(&format!("{}-result", language_key(language)))
        .or_else(|| directive.value("result"))
    else {
        return Ok(None);
    };

    let Some(args_field) = directive.value("args-field") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.function".to_string(),
            reason: "missing required `args-field`".to_string(),
        });
    };

    let primary = directive
        .value("primary")
        .map(parse_bool)
        .transpose()
        .map_err(|reason| Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.function".to_string(),
            reason,
        })?
        .unwrap_or(false);

    Ok(Some(FunctionFieldSpec {
        primary,
        result_type: result_type.to_string(),
        args_field: args_field.to_snake_case(),
    }))
}

fn build_with_arguments_field(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
) -> Result<Option<WithArgumentsFieldSpec>> {
    if language != Language::TypeScript {
        return Ok(None);
    }

    let Some(directive) = directive(directives, "with-arguments", path, context)? else {
        return Ok(None);
    };

    let Some(args_field) = directive.value("args-field") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `args-field`".to_string(),
        });
    };
    let Some(value_type) = directive.value("value-type") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `value-type`".to_string(),
        });
    };
    let Some(args_type) = directive.value("args-type") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `args-type`".to_string(),
        });
    };
    let Some(name_expr) = directive.value("name-expr") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `name-expr`".to_string(),
        });
    };

    Ok(Some(WithArgumentsFieldSpec {
        args_field: args_field.to_snake_case(),
        value_type: value_type.to_string(),
        args_type: args_type.to_string(),
        name_expr: name_expr.to_string(),
    }))
}

fn build_service(
    language: Language,
    _resolve: &Resolve,
    key: &WorldKey,
    interface: &Interface,
    path: &Path,
) -> Result<ServiceSpec> {
    let interface_name = interface_export_name(key, interface);
    let context = format!("interface `{interface_name}`");
    let directives = parse_directives(interface.docs.contents.as_deref(), path, &context)?;
    let endpoint = directive_value(&directives, "endpoint", path, &context, "value")?;
    let service_name = interface_name.to_upper_camel_case();

    let operations = interface
        .functions
        .iter()
        .map(|(_, function)| build_operation(language, function, path, &context, &service_name))
        .collect::<Result<Vec<_>>>()?;

    Ok(ServiceSpec {
        name: service_name,
        endpoint,
        operations,
    })
}

fn build_operation(
    language: Language,
    function: &Function,
    path: &Path,
    service_context: &str,
    service_name: &str,
) -> Result<OperationSpec> {
    let operation_name = function.name.to_upper_camel_case();
    let context = format!("{service_context} operation `{operation_name}`");
    let directives = parse_directives(function.docs.contents.as_deref(), path, &context)?;

    let input_ref = find_language_directive_value(
        language,
        &[function.docs.contents.as_deref()],
        path,
        &context,
        "input-ref",
    )?
    .ok_or_else(|| Error::InvalidWitDirective {
        path: path.to_path_buf(),
        context: context.clone(),
        directive: "@nexus.input-ref".to_string(),
        reason: "missing selected-language value".to_string(),
    })?;
    let output_ref = find_language_directive_value(
        language,
        &[function.docs.contents.as_deref()],
        path,
        &context,
        "output-ref",
    )?
    .ok_or_else(|| Error::InvalidWitDirective {
        path: path.to_path_buf(),
        context: context.clone(),
        directive: "@nexus.output-ref".to_string(),
        reason: "missing selected-language value".to_string(),
    })?;

    let output_transform = build_operation_output_transform(
        language,
        &directives,
        path,
        &context,
        service_name,
        &operation_name,
    )?;

    Ok(OperationSpec {
        name: operation_name,
        input_ref,
        output_ref,
        output_transform,
    })
}

fn build_operation_output_transform(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
    service_name: &str,
    operation_name: &str,
) -> Result<Option<OperationOutputTransformSpec>> {
    let Some(directive) = directive(directives, "output-transform", path, context)? else {
        return Ok(None);
    };

    let type_key = format!("{}-type", language_key(language));
    let type_name = directive.value(&type_key);
    let transform = directive_language_value(directive, language);

    match (type_name, transform) {
        (None, None) => Ok(None),
        (Some(type_name), Some(transform)) => Ok(Some(OperationOutputTransformSpec {
            type_name: type_name.to_string(),
            transform: transform.to_string(),
        })),
        _ => Err(Error::IncompleteOperationOutputTransform {
            service: service_name.to_string(),
            operation: operation_name.to_string(),
        }),
    }
}

fn select_world(
    resolve: &Resolve,
    package_id: PackageId,
    path: &Path,
) -> Result<wit_parser::WorldId> {
    let package = &resolve.packages[package_id];
    match package.worlds.len() {
        1 => Ok(*package
            .worlds
            .values()
            .next()
            .expect("world map length checked")),
        0 => Err(Error::InvalidWit {
            path: path.to_path_buf(),
            reason: "package must declare exactly one world".to_string(),
        }),
        _ => Err(Error::InvalidWit {
            path: path.to_path_buf(),
            reason: "package declares multiple worlds; choose one world per input".to_string(),
        }),
    }
}

fn interface_export_name(key: &WorldKey, interface: &Interface) -> String {
    match key {
        WorldKey::Name(name) => name.clone(),
        WorldKey::Interface(_) => interface
            .name
            .clone()
            .unwrap_or_else(|| "unnamed-interface".to_string()),
    }
}

fn is_optional_type(resolve: &Resolve, ty: &Type) -> bool {
    let mut current = ty;
    loop {
        match current {
            Type::Id(id) => match &resolve.types[*id].kind {
                TypeDefKind::Option(_) => return true,
                TypeDefKind::Type(next) => current = next,
                _ => return false,
            },
            _ => return false,
        }
    }
}

fn find_language_directive_value(
    language: Language,
    docs: &[Option<&str>],
    path: &Path,
    context: &str,
    directive_name: &str,
) -> Result<Option<String>> {
    for docs in docs {
        let directives = parse_directives(*docs, path, context)?;
        if let Some(value) =
            directive_value_for_language(&directives, directive_name, path, context, language)?
        {
            return Ok(Some(value));
        }
    }

    Ok(None)
}

fn directive_value_for_language(
    directives: &[Directive],
    name: &str,
    path: &Path,
    context: &str,
    language: Language,
) -> Result<Option<String>> {
    let Some(directive) = directive(directives, name, path, context)? else {
        return Ok(None);
    };
    Ok(directive_language_value(directive, language)
        .or_else(|| directive.value("value"))
        .map(ToOwned::to_owned))
}

fn directive_value(
    directives: &[Directive],
    name: &str,
    path: &Path,
    context: &str,
    key: &str,
) -> Result<Option<String>> {
    Ok(directive(directives, name, path, context)?
        .and_then(|directive| directive.value(key))
        .map(ToOwned::to_owned))
}

fn directive<'a>(
    directives: &'a [Directive],
    name: &str,
    path: &Path,
    context: &str,
) -> Result<Option<&'a Directive>> {
    let mut matches = directives.iter().filter(|directive| directive.name == name);
    let first = matches.next();
    if matches.next().is_some() {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: format!("@nexus.{name}"),
            reason: "duplicate directive".to_string(),
        });
    }
    Ok(first)
}

fn directive_language_value<'a>(directive: &'a Directive, language: Language) -> Option<&'a str> {
    directive.value(language_key(language))
}

fn language_key(language: Language) -> &'static str {
    match language {
        Language::Dotnet => "dotnet",
        Language::Go => "go",
        Language::Java => "java",
        Language::Python => "python",
        Language::Ruby => "ruby",
        Language::TypeScript => "typescript",
    }
}

fn parse_directives(docs: Option<&str>, path: &Path, context: &str) -> Result<Vec<Directive>> {
    let Some(docs) = docs else {
        return Ok(Vec::new());
    };

    docs.lines()
        .filter(|line| line.trim_start().starts_with("@nexus."))
        .map(|line| parse_directive_line(line.trim(), path, context))
        .collect()
}

#[derive(Debug, Clone)]
struct Directive {
    name: String,
    args: BTreeMap<String, String>,
}

impl Directive {
    fn value(&self, key: &str) -> Option<&str> {
        self.args.get(key).map(String::as_str)
    }
}

fn parse_directive_line(line: &str, path: &Path, context: &str) -> Result<Directive> {
    let Some(rest) = line.strip_prefix("@nexus.") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: line.to_string(),
            reason: "directive must start with `@nexus.`".to_string(),
        });
    };

    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = &rest[..name_end];
    let mut tail = rest[name_end..].trim_start();
    let mut args = BTreeMap::new();

    if tail.starts_with('"') {
        let (value, remaining) = parse_directive_value(tail, path, context, name)?;
        args.insert("value".to_string(), value);
        tail = remaining.trim_start();
    }

    while !tail.is_empty() {
        let key_end = tail
            .find(|character: char| character == '=' || character.is_whitespace())
            .unwrap_or(tail.len());
        let key = &tail[..key_end];
        let after_key = tail[key_end..].trim_start();
        let Some(after_equals) = after_key.strip_prefix('=') else {
            return Err(Error::InvalidWitDirective {
                path: path.to_path_buf(),
                context: context.to_string(),
                directive: format!("@nexus.{name}"),
                reason: format!("expected `=` after `{key}`"),
            });
        };
        let (value, remaining) =
            parse_directive_value(after_equals.trim_start(), path, context, name)?;
        args.insert(key.to_string(), value);
        tail = remaining.trim_start();
    }

    Ok(Directive {
        name: name.to_string(),
        args,
    })
}

fn parse_directive_value<'a>(
    input: &'a str,
    path: &Path,
    context: &str,
    name: &str,
) -> Result<(String, &'a str)> {
    if let Some(stripped) = input.strip_prefix('"') {
        let mut escaped = false;
        let mut value = String::new();
        for (index, character) in stripped.char_indices() {
            if escaped {
                value.push(character);
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => return Ok((value, &stripped[index + 1..])),
                _ => value.push(character),
            }
        }

        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: format!("@nexus.{name}"),
            reason: "unterminated quoted string".to_string(),
        });
    }

    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    Ok((input[..end].to_string(), &input[end..]))
}

fn parse_bool(value: &str) -> std::result::Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected `true` or `false`, found `{value}`")),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::descriptors::DescriptorIndex;
    use crate::error::Error;
    use crate::language::Language;

    use super::ApiSpec;

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn descriptors() -> DescriptorIndex {
        DescriptorIndex::load(&root().join("descriptors.bin")).unwrap()
    }

    fn parse(language: Language, wit: &str) -> ApiSpec {
        ApiSpec::parse_for_language(language, wit, PathBuf::from("inline.wit")).unwrap()
    }

    fn validate(language: Language, wit: &str) -> Result<(), Error> {
        let spec = parse(language, wit);
        let descriptors = descriptors();
        crate::validation::validate_type_overrides(&spec, &descriptors, language)
    }

    #[test]
    fn parses_wit_into_selected_language_spec() {
        let wit = r#"
/// @nexus.support python="python-validation/model_overrides.py" typescript="typescript-validation/model_overrides.ts"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

/// @nexus.endpoint "__temporal_system"
interface workflow-service {
  /// @nexus.proto "temporal.api.common.v1.RetryPolicy"
  /// @nexus.type python="temporalio.common.RetryPolicy" typescript="common.RetryPolicy"
  type retry-policy = string;

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_type"
    /// @nexus.function primary=true args-field="input" python-result="collections.abc.Awaitable[typing.Any]" typescript-result="Promise<any>"
    workflow: string,
    input: option<string>,
    workflow-id: string,
    task-queue: string,
    /// @nexus.proto-field "signal_name"
    /// @nexus.function args-field="signal-input" python-result="None | collections.abc.Awaitable[None]"
    /// @nexus.with-arguments args-field="signal-input" value-type="workflow.SignalDefinition<any[]>" args-type="Value extends workflow.SignalDefinition<infer Args, any> ? Args : never" name-expr="value.name"
    signal: string,
    signal-input: option<string>,
    /// @nexus.source python="workflow.info().namespace" typescript="workflow.workflowInfo().namespace"
    namespace: option<string>,
  }

  /// @nexus.input-ref python="temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest" typescript="@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionRequest"
  /// @nexus.output-ref python="temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse" typescript="@temporalio/api/workflowservice/v1.SignalWithStartWorkflowExecutionResponse"
  /// @nexus.output-transform python-type="workflow.ExternalWorkflowHandle[typing.Any]" python="workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)" typescript-type="workflow.ExternalWorkflowHandle" typescript="workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)"
  signal-with-start-workflow-execution: func(
    request: signal-with-start-workflow-execution-request
  ) -> string;
}
"#;

        let python = parse(Language::Python, wit);
        let typescript = parse(Language::TypeScript, wit);

        assert_eq!(
            python.support.file.as_deref(),
            Some("python-validation/model_overrides.py")
        );
        assert_eq!(
            typescript.support.file.as_deref(),
            Some("typescript-validation/model_overrides.ts")
        );

        let request = python
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap();
        assert!(request.is_field_required("workflow_type"));
        assert!(request.is_field_hidden("header"));
        let model = request.generated_model().unwrap();
        assert_eq!(model.field_name_override("workflow_type"), Some("workflow"));
        assert_eq!(
            model.field_name_override("workflow_id"),
            Some("workflow-id")
        );
        assert!(model.function("workflow_type").unwrap().primary);
        assert_eq!(
            model.field_source("namespace"),
            Some("workflow.info().namespace")
        );

        let typescript_model = typescript
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap()
            .generated_model()
            .unwrap();
        assert!(typescript_model.function("workflow_type").is_some());
        assert!(typescript_model.with_arguments("signal_name").is_some());
        assert!(typescript_model.function("signal_name").is_none());
    }

    #[test]
    fn validates_wit_function_fields() {
        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

/// @nexus.endpoint "__temporal_system"
interface workflow-service {
  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_type"
    /// @nexus.function primary=true args-field="input" python-result="collections.abc.Awaitable[typing.Any]"
    workflow: string,
    input: option<string>,
    workflow-id: string,
    task-queue: string,
    /// @nexus.proto-field "signal_name"
    signal: string,
  }

  /// @nexus.input-ref python="temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  /// @nexus.output-ref python="temporalio.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
  signal-with-start-workflow-execution: func(
    request: signal-with-start-workflow-execution-request
  ) -> string;
}
"#;

        validate(Language::Python, wit).unwrap();
    }

    #[test]
    fn rejects_duplicate_proto_field_mappings() {
        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

interface workflow-service {
  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_id"
    workflow-id: string,
    /// @nexus.proto-field "workflow_id"
    workflow-id-alias: string,
  }
}
"#;

        let err = ApiSpec::parse_for_language(Language::Python, wit, PathBuf::from("inline.wit"))
            .unwrap_err();
        assert!(matches!(err, Error::InvalidWit { .. }));
    }
}
