use std::collections::HashMap;
use std::fs;
use std::path::Path;

use heck::ToKebabCase;
use prost::Message;
use prost_types::{
    DescriptorProto, EnumDescriptorProto, FileDescriptorProto, FileDescriptorSet,
    MethodDescriptorProto, ServiceDescriptorProto,
};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct DescriptorIndex {
    files: Vec<FileDescriptorProto>,
    messages: HashMap<String, MessageMetadata>,
    enums: HashMap<String, EnumMetadata>,
    rpcs: Vec<RpcMetadata>,
}

impl DescriptorIndex {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).map_err(|source| Error::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        let set = FileDescriptorSet::decode(bytes.as_slice()).map_err(|source| {
            Error::DescriptorDecode {
                path: path.to_path_buf(),
                source,
            }
        })?;

        Ok(Self::from_descriptor_set(set))
    }

    pub fn from_descriptor_set(set: FileDescriptorSet) -> Self {
        let files = set.file;
        let mut messages = HashMap::new();
        let mut enums = HashMap::new();
        let mut rpcs = Vec::new();

        for file in &files {
            let package = file.package.as_deref().unwrap_or_default();
            for service in &file.service {
                index_service(file, package, service, &mut rpcs);
            }
            for enumeration in &file.enum_type {
                index_enum(file, package, None, enumeration, &mut enums);
            }
            for message in &file.message_type {
                index_message(file, package, None, message, &mut messages, &mut enums);
            }
        }

        Self {
            files,
            messages,
            enums,
            rpcs,
        }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn message(&self, full_name: &str) -> Option<&MessageMetadata> {
        let normalized = full_name.trim_start_matches('.');
        self.messages.get(normalized)
    }

    pub fn enumeration(&self, full_name: &str) -> Option<&EnumMetadata> {
        let normalized = full_name.trim_start_matches('.');
        self.enums.get(normalized)
    }

    pub fn resolve_rpc(&self, query: &str) -> Result<&RpcMetadata> {
        let normalized_query = normalize_identifier(query);

        let exact_matches = self
            .rpcs
            .iter()
            .filter(|rpc| rpc.matches_exact(&normalized_query))
            .collect::<Vec<_>>();
        if let [rpc] = exact_matches.as_slice() {
            return Ok(*rpc);
        }
        if !exact_matches.is_empty() {
            return Err(Error::AmbiguousRpcName {
                name: query.to_string(),
                matches: exact_matches
                    .iter()
                    .map(|rpc| rpc.full_name.clone())
                    .collect(),
            });
        }

        let fuzzy_matches = self
            .rpcs
            .iter()
            .filter(|rpc| rpc.matches_fuzzy(query))
            .collect::<Vec<_>>();
        if let [rpc] = fuzzy_matches.as_slice() {
            return Ok(*rpc);
        }
        if !fuzzy_matches.is_empty() {
            return Err(Error::AmbiguousRpcName {
                name: query.to_string(),
                matches: fuzzy_matches
                    .iter()
                    .map(|rpc| rpc.full_name.clone())
                    .collect(),
            });
        }

        Err(Error::UnknownRpcName {
            name: query.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MessageMetadata {
    pub full_name: String,
    pub file_name: Option<String>,
    pub package: String,
    pub descriptor: DescriptorProto,
}

#[derive(Debug, Clone)]
pub struct EnumMetadata {
    pub full_name: String,
    pub file_name: Option<String>,
    pub package: String,
    pub descriptor: EnumDescriptorProto,
}

#[derive(Debug, Clone)]
pub struct RpcMetadata {
    pub full_name: String,
    pub name: String,
    pub service_name: String,
    pub service_full_name: String,
    pub input_type: String,
    pub output_type: String,
    pub file_name: Option<String>,
    pub package: String,
    pub descriptor: MethodDescriptorProto,
}

impl RpcMetadata {
    fn matches_exact(&self, normalized_query: &str) -> bool {
        [
            normalize_identifier(&self.full_name),
            normalize_identifier(&self.service_method_name()),
            normalize_identifier(&self.name),
        ]
        .into_iter()
        .any(|candidate| candidate == normalized_query)
    }

    fn matches_fuzzy(&self, query: &str) -> bool {
        let query_tokens = kebab_tokens(query);
        !query_tokens.is_empty()
            && (tokens_are_subsequence(&query_tokens, &kebab_tokens(&self.name))
                || tokens_are_subsequence(
                    &query_tokens,
                    &kebab_tokens(&self.service_method_name()),
                )
                || tokens_are_subsequence(&query_tokens, &kebab_tokens(&self.full_name)))
    }

    fn service_method_name(&self) -> String {
        format!("{}.{}", self.service_name, self.name)
    }
}

fn index_message(
    file: &FileDescriptorProto,
    package: &str,
    parent: Option<&str>,
    descriptor: &DescriptorProto,
    messages: &mut HashMap<String, MessageMetadata>,
    enums: &mut HashMap<String, EnumMetadata>,
) {
    let Some(name) = descriptor.name.as_deref() else {
        return;
    };

    let full_name = if let Some(parent) = parent {
        format!("{parent}.{name}")
    } else if package.is_empty() {
        name.to_string()
    } else {
        format!("{package}.{name}")
    };

    messages.insert(
        full_name.clone(),
        MessageMetadata {
            full_name: full_name.clone(),
            file_name: file.name.clone(),
            package: package.to_string(),
            descriptor: descriptor.clone(),
        },
    );

    for enumeration in &descriptor.enum_type {
        index_enum(file, package, Some(&full_name), enumeration, enums);
    }

    for nested in &descriptor.nested_type {
        index_message(file, package, Some(&full_name), nested, messages, enums);
    }
}

fn index_enum(
    file: &FileDescriptorProto,
    package: &str,
    parent: Option<&str>,
    descriptor: &EnumDescriptorProto,
    enums: &mut HashMap<String, EnumMetadata>,
) {
    let Some(name) = descriptor.name.as_deref() else {
        return;
    };

    let full_name = if let Some(parent) = parent {
        format!("{parent}.{name}")
    } else if package.is_empty() {
        name.to_string()
    } else {
        format!("{package}.{name}")
    };

    enums.insert(
        full_name.clone(),
        EnumMetadata {
            full_name,
            file_name: file.name.clone(),
            package: package.to_string(),
            descriptor: descriptor.clone(),
        },
    );
}

fn index_service(
    file: &FileDescriptorProto,
    package: &str,
    descriptor: &ServiceDescriptorProto,
    rpcs: &mut Vec<RpcMetadata>,
) {
    let Some(service_name) = descriptor.name.as_deref() else {
        return;
    };

    let service_full_name = if package.is_empty() {
        service_name.to_string()
    } else {
        format!("{package}.{service_name}")
    };

    for method in &descriptor.method {
        let Some(name) = method.name.as_deref() else {
            continue;
        };

        rpcs.push(RpcMetadata {
            full_name: format!("{service_full_name}.{name}"),
            name: name.to_string(),
            service_name: service_name.to_string(),
            service_full_name: service_full_name.clone(),
            input_type: method.input_type.clone().unwrap_or_default(),
            output_type: method.output_type.clone().unwrap_or_default(),
            file_name: file.name.clone(),
            package: package.to_string(),
            descriptor: method.clone(),
        });
    }
}

fn normalize_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn kebab_tokens(value: &str) -> Vec<String> {
    value
        .trim_start_matches('.')
        .replace('.', "-")
        .to_kebab_case()
        .split('-')
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn tokens_are_subsequence(query: &[String], candidate: &[String]) -> bool {
    if query.is_empty() {
        return true;
    }

    let mut index = 0usize;
    for token in candidate {
        if token == &query[index] {
            index += 1;
            if index == query.len() {
                return true;
            }
        }
    }

    false
}
