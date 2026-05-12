use std::collections::HashMap;
use std::fs;
use std::path::Path;

use prost::Message;
use prost_types::{DescriptorProto, EnumDescriptorProto, FileDescriptorProto, FileDescriptorSet};

use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct DescriptorIndex {
    files: Vec<FileDescriptorProto>,
    messages: HashMap<String, MessageMetadata>,
    enums: HashMap<String, EnumMetadata>,
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

        for file in &files {
            let package = file.package.as_deref().unwrap_or_default();
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

    pub(crate) fn messages_with_suffix(&self, suffix: &str) -> Vec<&MessageMetadata> {
        self.messages
            .values()
            .filter(|message| message.full_name.ends_with(suffix))
            .collect()
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
