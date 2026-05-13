use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use heck::ToKebabCase;
use prost_types::FieldDescriptorProto;
use prost_types::field_descriptor_proto::{Label, Type};
use wit_parser::{Interface, Resolve, WorldItem, WorldKey};

use crate::descriptors::{DescriptorIndex, EnumMetadata, MessageMetadata, RpcMetadata};
use crate::error::{Error, Result};
use crate::spec::{
    BuiltinWitMetadata, find_proto_name_for_type_def, load_builtin_wit_metadata,
    parse_wit_with_builtins, select_world,
};

const DEFAULT_PACKAGE_NAME: &str = "temporal:nexus@1.0.0";
const DEFAULT_WORLD_NAME: &str = "system";
const DEFAULT_ENDPOINT_PLACEHOLDER: &str = "__REPLACE_ME__";

pub fn generate_add_rpc_wit(descriptors: &DescriptorIndex, rpc_name: &str) -> Result<String> {
    let rpc = descriptors.resolve_rpc(rpc_name)?;
    let builtin_wit = load_builtin_wit_metadata()?;
    AddRpcBuilder::new(descriptors, rpc, builtin_wit)
        .build()
        .map(|rendered| rendered.render_standalone())
}

pub fn generate_add_rpc_wit_with_input(
    descriptors: &DescriptorIndex,
    rpc_name: &str,
    input_path: &Path,
    input: &str,
) -> Result<String> {
    let rpc = descriptors.resolve_rpc(rpc_name)?;
    let builtin_wit = load_builtin_wit_metadata()?;
    let existing = ExistingWitDocument::load(input_path, input)?;
    let interface_name = rpc.service_name.to_kebab_case();
    let operation_name = rpc.name.to_kebab_case();

    if let Some(interface) = existing.interfaces.get(&interface_name) {
        if interface.function_names.contains(&operation_name) {
            return Ok(input.to_string());
        }

        let rendered = AddRpcBuilder::new(descriptors, rpc, builtin_wit)
            .with_existing_interface(interface)
            .build()?;
        let snippet = rendered.render_interface_items();
        return insert_into_named_block(input, "interface", &interface_name, &snippet);
    }

    let rendered = AddRpcBuilder::new(descriptors, rpc, builtin_wit).build()?;
    let source = insert_world_export(input, &existing.world_name, &interface_name)?;
    let interface_block = rendered.render_new_interface_block();
    insert_after_named_block(&source, "world", &existing.world_name, &interface_block)
}

#[derive(Debug, Clone)]
struct ExistingWitDocument {
    world_name: String,
    interfaces: BTreeMap<String, ExistingInterface>,
}

impl ExistingWitDocument {
    fn load(path: &Path, input: &str) -> Result<Self> {
        let parsed = parse_wit_with_builtins(input, path)?;
        let world_id = select_world(&parsed.resolve, parsed.package_id, path)?;
        let world = &parsed.resolve.worlds[world_id];

        let mut interfaces = BTreeMap::new();
        for (key, item) in &world.exports {
            let WorldItem::Interface { id, .. } = item else {
                continue;
            };
            let interface = &parsed.resolve.interfaces[*id];
            let export_name = exported_interface_name(key, interface);
            let interface_source = find_named_block(input, "interface", &export_name)
                .map(|block| &input[block.brace_start + 1..block.end_start]);
            interfaces.insert(
                export_name.clone(),
                ExistingInterface::from_resolve(
                    &parsed.resolve,
                    interface,
                    export_name,
                    path,
                    interface_source,
                )?,
            );
        }

        Ok(Self {
            world_name: world.name.clone(),
            interfaces,
        })
    }
}

#[derive(Debug, Clone, Default)]
struct ExistingInterface {
    function_names: BTreeSet<String>,
    type_names_by_proto: BTreeMap<String, String>,
    type_names_in_scope: BTreeSet<String>,
}

impl ExistingInterface {
    fn from_resolve(
        resolve: &Resolve,
        interface: &Interface,
        export_name: String,
        path: &Path,
        interface_source: Option<&str>,
    ) -> Result<Self> {
        let function_names = interface.functions.keys().cloned().collect();
        let mut type_names_by_proto = BTreeMap::new();
        let mut type_names_in_scope = BTreeSet::new();

        for (type_name, type_id) in &interface.types {
            type_names_in_scope.insert(type_name.clone());
            let type_def = &resolve.types[*type_id];
            let context = format!("interface `{export_name}` type `{type_name}`");
            let Some(proto_name) = find_proto_name_for_type_def(type_def, path, &context)? else {
                continue;
            };
            type_names_by_proto.insert(proto_name, type_name.clone());
        }
        if let Some(interface_source) = interface_source {
            type_names_in_scope.extend(collect_used_type_names(interface_source));
        }

        Ok(Self {
            function_names,
            type_names_by_proto,
            type_names_in_scope,
        })
    }
}

struct AddRpcBuilder<'a> {
    descriptors: &'a DescriptorIndex,
    rpc: &'a RpcMetadata,
    builtin_wit: BuiltinWitMetadata,
    builtin_uses: BTreeMap<String, BTreeSet<String>>,
    available_type_names: BTreeMap<String, String>,
    reserved_type_names: BTreeSet<String>,
    rendered_types: BTreeSet<String>,
    rendered_definitions: Vec<String>,
}

impl<'a> AddRpcBuilder<'a> {
    fn new(
        descriptors: &'a DescriptorIndex,
        rpc: &'a RpcMetadata,
        builtin_wit: BuiltinWitMetadata,
    ) -> Self {
        Self {
            descriptors,
            rpc,
            builtin_wit,
            builtin_uses: BTreeMap::new(),
            available_type_names: BTreeMap::new(),
            reserved_type_names: BTreeSet::new(),
            rendered_types: BTreeSet::new(),
            rendered_definitions: Vec::new(),
        }
    }

    fn with_existing_interface(mut self, interface: &ExistingInterface) -> Self {
        self.available_type_names = interface.type_names_by_proto.clone();
        self.reserved_type_names = interface.type_names_in_scope.clone();
        self
    }

    fn build(mut self) -> Result<RenderedAddRpcWit> {
        let input_type = self.render_type_reference(&self.rpc.input_type, &self.rpc.full_name)?;
        let output_type = self.render_type_reference(&self.rpc.output_type, &self.rpc.full_name)?;

        Ok(RenderedAddRpcWit {
            rpc_full_name: self.rpc.full_name.clone(),
            interface_name: self.rpc.service_name.to_kebab_case(),
            builtin_uses: self.builtin_uses,
            rendered_definitions: self.rendered_definitions,
            operation: format!(
                "  {}: func(\n    request: {},\n  ) -> {};\n",
                self.rpc.name.to_kebab_case(),
                input_type,
                output_type
            ),
        })
    }

    fn render_type_reference(&mut self, proto_name: &str, context: &str) -> Result<String> {
        let proto_name = proto_name.trim_start_matches('.');
        if let Some(existing_name) = self.available_type_names.get(proto_name) {
            return Ok(existing_name.clone());
        }

        if let Some(builtin) = self.builtin_wit.proto_types.get(proto_name).cloned() {
            self.use_builtin_type(proto_name, &builtin.wit_name)?;
            return Ok(builtin.wit_name);
        }

        if let Some(message) = self.descriptors.message(proto_name) {
            let wit_name = self.reserve_local_type_name(proto_name, context)?;
            if self.rendered_types.insert(proto_name.to_string()) {
                let definition = self.render_message(message, &wit_name)?;
                self.rendered_definitions.push(definition);
            }
            return Ok(wit_name);
        }

        if let Some(enumeration) = self.descriptors.enumeration(proto_name) {
            let wit_name = self.reserve_local_type_name(proto_name, context)?;
            if self.rendered_types.insert(proto_name.to_string()) {
                let definition = self.render_enum(enumeration, &wit_name)?;
                self.rendered_definitions.push(definition);
            }
            return Ok(wit_name);
        }

        Err(Error::UnsupportedAddRpc {
            context: context.to_string(),
            reason: format!("unknown proto type `{proto_name}`"),
        })
    }

    fn render_message(&mut self, message: &MessageMetadata, wit_name: &str) -> Result<String> {
        let mut rendered_fields = Vec::new();
        for field in &message.descriptor.field {
            let field_name = field_name(field, &message.full_name)?;
            let wit_field_type = self.render_field_type(field, &message.full_name, field_name)?;
            rendered_fields.push(format!(
                "    {}: {},",
                field_name.to_kebab_case(),
                wit_field_type
            ));
        }

        let mut rendered = String::new();
        rendered.push_str(&format!("  /// @nexus.proto \"{}\"\n", message.full_name));
        rendered.push_str(&format!("  record {wit_name} {{\n"));
        for field in rendered_fields {
            rendered.push_str(&field);
            rendered.push('\n');
        }
        rendered.push_str("  }\n");
        Ok(rendered)
    }

    fn render_enum(&mut self, enumeration: &EnumMetadata, wit_name: &str) -> Result<String> {
        let mut rendered = String::new();
        rendered.push_str(&format!(
            "  /// @nexus.proto \"{}\"\n",
            enumeration.full_name
        ));
        rendered.push_str(&format!("  enum {wit_name} {{\n"));
        for value in &enumeration.descriptor.value {
            let Some(name) = value.name.as_deref() else {
                return Err(Error::UnsupportedAddRpc {
                    context: enumeration.full_name.clone(),
                    reason: "enum value is missing a name".to_string(),
                });
            };
            rendered.push_str(&format!("    {},\n", name.to_kebab_case()));
        }
        rendered.push_str("  }\n");
        Ok(rendered)
    }

    fn render_field_type(
        &mut self,
        field: &FieldDescriptorProto,
        parent_type: &str,
        field_name: &str,
    ) -> Result<String> {
        let context = format!("{parent_type}.{field_name}");
        let label =
            Label::try_from(field.label.unwrap_or(Label::Optional as i32)).map_err(|_| {
                Error::UnsupportedAddRpc {
                    context: context.clone(),
                    reason: "unknown field label".to_string(),
                }
            })?;
        let field_type = Type::try_from(field.r#type.unwrap_or_default()).map_err(|_| {
            Error::UnsupportedAddRpc {
                context: context.clone(),
                reason: "unknown field type".to_string(),
            }
        })?;

        let base_type = match field_type {
            Type::Double => "f64".to_string(),
            Type::Float => "f32".to_string(),
            Type::Int64 | Type::Sint64 | Type::Sfixed64 => "s64".to_string(),
            Type::Uint64 | Type::Fixed64 => "u64".to_string(),
            Type::Int32 | Type::Sint32 | Type::Sfixed32 => "s32".to_string(),
            Type::Uint32 | Type::Fixed32 => "u32".to_string(),
            Type::Bool => "bool".to_string(),
            Type::String => "string".to_string(),
            Type::Bytes => "list<u8>".to_string(),
            Type::Message | Type::Group | Type::Enum => {
                let Some(type_name) = field.type_name.as_deref() else {
                    return Err(Error::UnsupportedAddRpc {
                        context,
                        reason: "field is missing a referenced type name".to_string(),
                    });
                };
                self.render_type_reference(type_name, parent_type)?
            }
        };

        if label == Label::Repeated {
            return Ok(format!("list<{base_type}>"));
        }

        if field_has_presence(field, field_type) {
            return Ok(format!("option<{base_type}>"));
        }

        Ok(base_type)
    }

    fn reserve_local_type_name(&mut self, proto_name: &str, context: &str) -> Result<String> {
        if let Some(existing_name) = self.available_type_names.get(proto_name) {
            return Ok(existing_name.clone());
        }

        let wit_name = self.local_type_name(proto_name);
        if self.reserved_type_names.contains(&wit_name) {
            return Err(Error::UnsupportedAddRpc {
                context: context.to_string(),
                reason: format!(
                    "generated type name `{wit_name}` would collide with an existing WIT type"
                ),
            });
        }

        self.available_type_names
            .insert(proto_name.to_string(), wit_name.clone());
        self.reserved_type_names.insert(wit_name.clone());
        Ok(wit_name)
    }

    fn use_builtin_type(&mut self, proto_name: &str, wit_name: &str) -> Result<()> {
        let Some(use_path) = self.builtin_wit.type_use_paths.get(wit_name) else {
            return Err(Error::UnsupportedAddRpc {
                context: self.rpc.full_name.clone(),
                reason: format!("built-in WIT type `{wit_name}` was not found in bundled metadata"),
            });
        };

        let already_in_scope = self.reserved_type_names.contains(wit_name);
        self.available_type_names
            .insert(proto_name.to_string(), wit_name.to_string());
        self.reserved_type_names.insert(wit_name.to_string());
        if !already_in_scope {
            self.builtin_uses
                .entry(use_path.clone())
                .or_default()
                .insert(wit_name.to_string());
        }
        Ok(())
    }

    fn local_type_name(&self, proto_name: &str) -> String {
        if let Some(message) = self.descriptors.message(proto_name) {
            return descriptor_relative_name(&message.full_name, &message.package);
        }
        if let Some(enumeration) = self.descriptors.enumeration(proto_name) {
            return descriptor_relative_name(&enumeration.full_name, &enumeration.package);
        }
        proto_name
            .trim_start_matches('.')
            .replace('.', "-")
            .to_kebab_case()
    }
}

struct RenderedAddRpcWit {
    rpc_full_name: String,
    interface_name: String,
    builtin_uses: BTreeMap<String, BTreeSet<String>>,
    rendered_definitions: Vec<String>,
    operation: String,
}

impl RenderedAddRpcWit {
    fn render_standalone(&self) -> String {
        let mut rendered = String::new();
        rendered.push_str(&format!(
            "/// WIT scaffold generated from `{}`.\n",
            self.rpc_full_name
        ));
        rendered.push_str(
            "/// Replace the endpoint and refine any inferred field mappings as needed.\n",
        );
        rendered.push_str(&format!("package {DEFAULT_PACKAGE_NAME};\n\n"));
        rendered.push_str(&format!("world {DEFAULT_WORLD_NAME} {{\n"));
        rendered.push_str(&format!("  export {};\n", self.interface_name));
        rendered.push_str("}\n\n");
        rendered.push_str(&self.render_new_interface_block());
        rendered
    }

    fn render_new_interface_block(&self) -> String {
        let mut rendered = String::new();
        rendered.push_str(&format!(
            "/// @nexus.endpoint \"{DEFAULT_ENDPOINT_PLACEHOLDER}\"\n"
        ));
        rendered.push_str(&format!("interface {} {{\n", self.interface_name));
        rendered.push_str(&self.render_interface_items());
        rendered.push_str("}\n");
        rendered
    }

    fn render_interface_items(&self) -> String {
        let mut rendered = String::new();

        if !self.builtin_uses.is_empty() {
            rendered.push_str(&self.render_builtin_use_block());
        }

        if !self.rendered_definitions.is_empty() {
            if !rendered.is_empty() {
                rendered.push('\n');
            }
            for (index, definition) in self.rendered_definitions.iter().enumerate() {
                rendered.push_str(definition);
                if index + 1 != self.rendered_definitions.len() {
                    rendered.push('\n');
                }
            }
        }

        if !rendered.is_empty() {
            rendered.push('\n');
        }
        rendered.push_str(&self.operation);
        rendered
    }

    fn render_builtin_use_block(&self) -> String {
        let mut rendered = String::new();
        for (use_path, builtins) in &self.builtin_uses {
            rendered.push_str(&format!("  use {use_path}.{{\n"));
            for builtin in builtins {
                rendered.push_str(&format!("    {builtin},\n"));
            }
            rendered.push_str("  };\n");
        }
        rendered
    }
}

#[derive(Debug, Clone, Copy)]
struct NamedBlock {
    brace_start: usize,
    end_start: usize,
    end_exclusive: usize,
}

fn insert_into_named_block(
    source: &str,
    keyword: &str,
    name: &str,
    snippet: &str,
) -> Result<String> {
    let Some(block) = find_named_block(source, keyword, name) else {
        return Err(Error::UnsupportedAddRpc {
            context: format!("{keyword} `{name}`"),
            reason: "existing WIT file does not contain the target block".to_string(),
        });
    };

    let mut rendered = String::with_capacity(source.len() + snippet.len() + 1);
    rendered.push_str(&source[..block.end_start]);
    rendered.push('\n');
    rendered.push_str(snippet);
    rendered.push_str(&source[block.end_start..]);
    Ok(rendered)
}

fn insert_world_export(source: &str, world_name: &str, export_name: &str) -> Result<String> {
    let Some(block) = find_named_block(source, "world", world_name) else {
        return Err(Error::UnsupportedAddRpc {
            context: format!("world `{world_name}`"),
            reason: "existing WIT file does not contain the target world".to_string(),
        });
    };

    let mut rendered = String::with_capacity(source.len() + export_name.len() + 12);
    rendered.push_str(&source[..block.end_start]);
    rendered.push_str(&format!("  export {export_name};\n"));
    rendered.push_str(&source[block.end_start..]);
    Ok(rendered)
}

fn insert_after_named_block(
    source: &str,
    keyword: &str,
    name: &str,
    snippet: &str,
) -> Result<String> {
    let Some(block) = find_named_block(source, keyword, name) else {
        return Err(Error::UnsupportedAddRpc {
            context: format!("{keyword} `{name}`"),
            reason: "existing WIT file does not contain the target block".to_string(),
        });
    };

    let mut rendered = String::with_capacity(source.len() + snippet.len() + 2);
    rendered.push_str(&source[..block.end_exclusive]);
    rendered.push_str("\n\n");
    rendered.push_str(snippet);
    rendered.push_str(&source[block.end_exclusive..]);
    Ok(rendered)
}

fn find_named_block(source: &str, keyword: &str, name: &str) -> Option<NamedBlock> {
    let needle = format!("{keyword} {name}");
    for (index, _) in source.match_indices(&needle) {
        if index > 0 && !source[..index].chars().next_back().unwrap().is_whitespace() {
            continue;
        }

        let after_name = index + needle.len();
        let brace_offset = source[after_name..].find('{')?;
        let brace_start = after_name + brace_offset;
        if !source[after_name..brace_start]
            .chars()
            .all(char::is_whitespace)
        {
            continue;
        }

        let end_start = find_matching_brace(source, brace_start)?;
        return Some(NamedBlock {
            brace_start,
            end_start,
            end_exclusive: end_start + 1,
        });
    }
    None
}

fn find_matching_brace(source: &str, open_brace: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, character) in source[open_brace..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_brace + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn exported_interface_name(key: &WorldKey, interface: &Interface) -> String {
    match key {
        WorldKey::Name(name) => name.clone(),
        WorldKey::Interface(_) => interface
            .name
            .clone()
            .unwrap_or_else(|| "unnamed-interface".to_string()),
    }
}

fn collect_used_type_names(interface_source: &str) -> BTreeSet<String> {
    let mut used_names = BTreeSet::new();
    let mut offset = 0usize;

    while let Some(use_index) = interface_source[offset..].find("use ") {
        let use_start = offset + use_index;
        let Some(brace_index) = interface_source[use_start..].find('{') else {
            break;
        };
        let brace_start = use_start + brace_index;
        let Some(brace_end_rel) = interface_source[brace_start + 1..].find('}') else {
            break;
        };
        let brace_end = brace_start + 1 + brace_end_rel;

        for name in interface_source[brace_start + 1..brace_end].split(',') {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let imported_name = name.split_whitespace().next().unwrap_or(name);
            used_names.insert(imported_name.to_string());
        }

        offset = brace_end + 1;
    }

    used_names
}

fn field_name<'a>(field: &'a FieldDescriptorProto, parent_type: &str) -> Result<&'a str> {
    field
        .name
        .as_deref()
        .ok_or_else(|| Error::UnsupportedAddRpc {
            context: parent_type.to_string(),
            reason: "field is missing a name".to_string(),
        })
}

fn field_has_presence(field: &FieldDescriptorProto, field_type: Type) -> bool {
    field.proto3_optional.unwrap_or(false)
        || field.oneof_index.is_some()
        || matches!(field_type, Type::Message | Type::Group)
}

fn descriptor_relative_name(full_name: &str, package: &str) -> String {
    let relative = full_name
        .trim_start_matches('.')
        .strip_prefix(&format!("{package}."))
        .unwrap_or(full_name.trim_start_matches('.'));
    relative.replace('.', "-").to_kebab_case()
}
