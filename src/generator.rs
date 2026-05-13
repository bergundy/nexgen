use crate::api_plan::build_api_plan;
use crate::SupportFiles;
use crate::descriptors::DescriptorIndex;
use crate::error::{Error, Result};
use crate::language::Language;
use crate::python;
use crate::resources::ensure_unique_resource_names;
use crate::spec::ApiSpec;
use crate::typescript;
use crate::validation::validate_type_overrides;

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

pub fn generate_source(
    language: Language,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    support: &SupportFiles,
) -> Result<String> {
    validate_type_overrides(spec, descriptors, language)?;
    ensure_unique_resource_names(spec)?;
    let plan = build_api_plan(spec, descriptors)?;

    match language {
        Language::Python => python::generate(&plan, support),
        Language::TypeScript => typescript::generate(&plan, support),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}
