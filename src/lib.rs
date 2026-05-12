pub mod descriptors;
pub mod error;
pub mod generator;
pub mod language;
pub mod python;
pub mod spec;
pub mod typescript;
pub mod validation;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
    pub format: bool,
}

pub fn generate_to_string(
    language: Language,
    input_path: impl AsRef<Path>,
    descriptor_path: impl AsRef<Path>,
) -> Result<String> {
    let input_path = input_path.as_ref();
    let spec = ApiSpec::load_for_language(language, input_path)?;
    let descriptors = DescriptorIndex::load(descriptor_path.as_ref())?;
    let support = load_support_files(language, &spec, input_path)?;
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

    if request.format {
        format_generated_file(request.language, &request.output_path)?;
    }

    Ok(())
}

fn format_generated_file(language: Language, output_path: &Path) -> Result<()> {
    let (program, args) = formatter_command(language, output_path)?;
    let command = format_formatter_command(program, &args);
    let status = Command::new(program)
        .args(&args)
        .status()
        .map_err(|source| error::Error::RunFormatter {
            path: output_path.to_path_buf(),
            command: command.clone(),
            source,
        })?;

    if !status.success() {
        return Err(error::Error::FormatterFailed {
            path: output_path.to_path_buf(),
            command,
            status,
        });
    }

    Ok(())
}

fn formatter_command(
    language: Language,
    output_path: &Path,
) -> Result<(&'static str, Vec<String>)> {
    let output_path = output_path.to_string_lossy().into_owned();
    match language {
        Language::Python => Ok(("ruff", vec!["format".to_string(), output_path])),
        Language::TypeScript => Ok(("prettier", vec!["--write".to_string(), output_path])),
        _ => Err(error::Error::UnsupportedLanguage { language }),
    }
}

fn format_formatter_command(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
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

fn load_support_files(
    language: Language,
    spec: &ApiSpec,
    input_path: &Path,
) -> Result<SupportFiles> {
    let base_dir = input_path.parent().unwrap_or_else(|| Path::new("."));
    let support_path = spec
        .support
        .file
        .as_deref()
        .map(|path| resolve_support_path(base_dir, path));
    let support_contents = load_optional_file(support_path.as_deref())?;

    Ok(match language {
        Language::Python => SupportFiles {
            python: support_contents,
            typescript: None,
        },
        Language::TypeScript => SupportFiles {
            python: None,
            typescript: support_contents,
        },
        _ => SupportFiles::default(),
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{format_formatter_command, formatter_command};
    use crate::language::Language;

    #[test]
    fn chooses_python_formatter_command() {
        let (program, args) = formatter_command(Language::Python, Path::new("output.py")).unwrap();
        assert_eq!(program, "ruff");
        assert_eq!(args, vec!["format", "output.py"]);
        assert_eq!(
            format_formatter_command(program, &args),
            "ruff format output.py"
        );
    }

    #[test]
    fn chooses_typescript_formatter_command() {
        let (program, args) =
            formatter_command(Language::TypeScript, Path::new("output.ts")).unwrap();
        assert_eq!(program, "prettier");
        assert_eq!(args, vec!["--write", "output.ts"]);
        assert_eq!(
            format_formatter_command(program, &args),
            "prettier --write output.ts"
        );
    }
}
