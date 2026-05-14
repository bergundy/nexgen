use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::SupportFiles;
use crate::api_plan::build_api_plan;
use crate::descriptors::DescriptorIndex;
use crate::error::{Error, Result};
use crate::language::Language;
use crate::python;
use crate::resources::ensure_unique_resource_names;
use crate::spec::ApiSpec;
use crate::typescript;
use crate::validation::validate_type_overrides;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedOutputLayout {
    SingleFile,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFiles {
    pub layout: GeneratedOutputLayout,
    pub files: BTreeMap<PathBuf, String>,
}

impl GeneratedFiles {
    pub fn single_file(contents: String) -> Self {
        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("output"), contents);
        Self {
            layout: GeneratedOutputLayout::SingleFile,
            files,
        }
    }

    pub fn directory(files: BTreeMap<PathBuf, String>) -> Self {
        Self {
            layout: GeneratedOutputLayout::Directory,
            files,
        }
    }

    pub fn single_file_contents(&self) -> Option<&str> {
        (self.layout == GeneratedOutputLayout::SingleFile)
            .then(|| self.files.values().next().map(String::as_str))
            .flatten()
    }
}

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

pub fn generate_files(
    language: Language,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    support: &SupportFiles,
) -> Result<GeneratedFiles> {
    validate_type_overrides(spec, descriptors, language)?;
    ensure_unique_resource_names(spec)?;
    let plan = build_api_plan(spec, descriptors)?;

    match language {
        Language::Python => python::generate(&plan, &spec.support.fragments),
        Language::TypeScript => typescript::generate(&plan, support),
        language => Err(Error::UnsupportedLanguage { language }),
    }
}

pub fn generate_source(
    language: Language,
    spec: &ApiSpec,
    descriptors: &DescriptorIndex,
    support: &SupportFiles,
) -> Result<String> {
    let generated = generate_files(language, spec, descriptors, support)?;
    Ok(match generated.layout {
        GeneratedOutputLayout::SingleFile => generated
            .single_file_contents()
            .expect("single-file output should contain one file")
            .to_string(),
        GeneratedOutputLayout::Directory => generated
            .files
            .iter()
            .map(|(path, contents)| format!("### {}\n{contents}", path.display()))
            .collect::<Vec<_>>()
            .join("\n\n"),
    })
}
