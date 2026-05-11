mod python;
mod typescript;

use std::collections::BTreeMap;

use crate::SupportFiles;
use crate::descriptors::{DescriptorIndex, MessageMetadata};
use crate::error::{Error, Result};
use crate::language::Language;
use crate::spec::{ApiSpec, Direction};

#[derive(Debug, Clone, Copy, Default)]
struct ModelCapabilities {
    from_proto: bool,
    to_proto: bool,
}

impl ModelCapabilities {
    const BIDIRECTIONAL: Self = Self {
        from_proto: true,
        to_proto: true,
    };
    const TO_PROTO_ONLY: Self = Self {
        from_proto: false,
        to_proto: true,
    };

    fn merge(&mut self, other: Self) {
        self.from_proto |= other.from_proto;
        self.to_proto |= other.to_proto;
    }
}

fn root_model_capabilities(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    language: Language,
) -> Result<BTreeMap<String, ModelCapabilities>> {
    let mut capabilities: BTreeMap<String, ModelCapabilities> = BTreeMap::new();

    for service in &spec.services {
        for operation in &service.operations {
            if let Some(reference) = operation.language_ref(language, Direction::Input) {
                let message = resolve_message_for_language(descriptors, language, reference)?;
                capabilities
                    .entry(message.full_name.clone())
                    .or_default()
                    .merge(ModelCapabilities::TO_PROTO_ONLY);
            }
            if let Some(reference) = operation.language_ref(language, Direction::Output) {
                let message = resolve_message_for_language(descriptors, language, reference)?;
                capabilities
                    .entry(message.full_name.clone())
                    .or_default()
                    .merge(ModelCapabilities::BIDIRECTIONAL);
            }
        }
    }

    Ok(capabilities)
}

fn resolve_message_for_language<'a>(
    descriptors: &'a DescriptorIndex,
    language: Language,
    reference: &str,
) -> Result<&'a MessageMetadata> {
    match language {
        Language::Python => descriptors.resolve_python_ref(reference),
        Language::TypeScript => descriptors.resolve_typescript_ref(reference),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}

pub trait GeneratorBackend {
    fn language(&self) -> Language;
    fn generate(
        &self,
        spec: &ApiSpec,
        descriptors: &DescriptorIndex,
        support: &SupportFiles,
    ) -> Result<String>;
}

pub fn generate_source(
    language: Language,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    support: &SupportFiles,
) -> Result<String> {
    spec.validate_type_overrides(descriptors)?;

    match language {
        Language::Python => python::PythonBackend.generate(spec, descriptors, support),
        Language::TypeScript => typescript::TypeScriptBackend.generate(spec, descriptors, support),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}
