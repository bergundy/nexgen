pub mod descriptors;
pub mod error;
pub mod generator;
pub mod language;
pub mod spec;

use std::fs;
use std::path::{Path, PathBuf};

use descriptors::DescriptorIndex;
use error::Result;
use generator::generate_source;
use language::Language;
use spec::ApiSpec;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportFiles {
    pub python: Option<String>,
}

pub struct GenerateRequest {
    pub language: Language,
    pub input_path: PathBuf,
    pub descriptor_path: PathBuf,
    pub output_path: PathBuf,
    pub python_support_path: Option<PathBuf>,
}

pub fn generate_to_string(
    language: Language,
    input_path: impl AsRef<Path>,
    descriptor_path: impl AsRef<Path>,
    python_support_path: Option<&Path>,
) -> Result<String> {
    let spec = ApiSpec::load(input_path.as_ref())?;
    let descriptors = DescriptorIndex::load(descriptor_path.as_ref())?;
    let support = SupportFiles {
        python: load_optional_file(python_support_path)?,
    };
    generate_source(language, &spec, &descriptors, &support)
}

pub fn generate_to_file(request: &GenerateRequest) -> Result<()> {
    let output = generate_to_string(
        request.language,
        &request.input_path,
        &request.descriptor_path,
        request.python_support_path.as_deref(),
    )?;

    fs::write(&request.output_path, output).map_err(|source| error::Error::WriteFile {
        path: request.output_path.clone(),
        source,
    })?;

    Ok(())
}

fn load_optional_file(path: Option<&Path>) -> Result<Option<String>> {
    path.map(|path| {
        fs::read_to_string(path).map_err(|source| error::Error::ReadFile {
            path: path.to_path_buf(),
            source,
        })
    })
    .transpose()
}
