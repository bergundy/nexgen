use std::collections::BTreeMap;

use crate::SupportFiles;
use crate::descriptors::DescriptorIndex;
use crate::error::{Error, Result};
use crate::language::Language;
use crate::python;
use crate::spec::{ApiSpec, Direction};
use crate::typescript;
use crate::validation::{resolve_message_for_language, validate_type_overrides};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ModelCapabilities {
    pub(crate) from_proto: bool,
    pub(crate) to_proto: bool,
}

impl ModelCapabilities {
    pub(crate) const BIDIRECTIONAL: Self = Self {
        from_proto: true,
        to_proto: true,
    };
    pub(crate) const TO_PROTO_ONLY: Self = Self {
        from_proto: false,
        to_proto: true,
    };

    pub(crate) fn merge(&mut self, other: Self) {
        self.from_proto |= other.from_proto;
        self.to_proto |= other.to_proto;
    }
}

pub(crate) fn root_model_capabilities(
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    language: Language,
) -> Result<BTreeMap<String, ModelCapabilities>> {
    let mut capabilities: BTreeMap<String, ModelCapabilities> = BTreeMap::new();

    for service in &spec.services {
        for operation in &service.operations {
            let input_message = resolve_message_for_language(
                descriptors,
                language,
                operation.reference(Direction::Input),
            )?;
            capabilities
                .entry(input_message.full_name.clone())
                .or_default()
                .merge(ModelCapabilities::TO_PROTO_ONLY);

            let output_message = resolve_message_for_language(
                descriptors,
                language,
                operation.reference(Direction::Output),
            )?;
            capabilities
                .entry(output_message.full_name.clone())
                .or_default()
                .merge(ModelCapabilities::BIDIRECTIONAL);
        }
    }

    Ok(capabilities)
}

pub fn generate_source(
    language: Language,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    support: &SupportFiles,
) -> Result<String> {
    validate_type_overrides(spec, descriptors, language)?;

    match language {
        Language::Python => python::generate(spec, descriptors, support),
        Language::TypeScript => typescript::generate(spec, descriptors, support),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}
