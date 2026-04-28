mod python;

use crate::SupportFiles;
use crate::descriptors::DescriptorIndex;
use crate::error::{Error, Result};
use crate::language::Language;
use crate::spec::ApiSpec;

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
    match language {
        Language::Python => python::PythonBackend.generate(spec, descriptors, support),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}
