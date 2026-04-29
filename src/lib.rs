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
    pub typescript: Option<String>,
}

pub struct GenerateRequest {
    pub language: Language,
    pub input_path: PathBuf,
    pub descriptor_path: PathBuf,
    pub output_path: PathBuf,
}

pub fn generate_to_string(
    language: Language,
    input_path: impl AsRef<Path>,
    descriptor_path: impl AsRef<Path>,
) -> Result<String> {
    let input_path = input_path.as_ref();
    let spec = ApiSpec::load(input_path)?;
    let descriptors = DescriptorIndex::load(descriptor_path.as_ref())?;
    let support = load_support_files(&spec, input_path)?;
    generate_source(language, &spec, &descriptors, &support)
}

pub fn generate_to_file(request: &GenerateRequest) -> Result<()> {
    let output = generate_to_string(
        request.language,
        &request.input_path,
        &request.descriptor_path,
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

fn load_support_files(spec: &ApiSpec, input_path: &Path) -> Result<SupportFiles> {
    let base_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let python_support_path = spec
        .support
        .python_file
        .as_deref()
        .map(|path| resolve_support_path(base_dir, path));
    let typescript_support_path = spec
        .support
        .typescript_file
        .as_deref()
        .map(|path| resolve_support_path(base_dir, path));

    Ok(SupportFiles {
        python: load_optional_file(python_support_path.as_deref())?,
        typescript: load_optional_file(typescript_support_path.as_deref())?,
    })
}

fn resolve_support_path(base_dir: &Path, support_path: &str) -> PathBuf {
    let support_path = PathBuf::from(support_path);
    if support_path.is_absolute() {
        support_path
    } else {
        base_dir.join(support_path)
    }
}
