mod api_plan;

pub mod add_rpc;
pub mod descriptors;
pub mod error;
pub mod generator;
pub mod language;
pub mod python;
pub mod resources;
pub mod spec;
pub mod typescript;
pub mod validation;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use add_rpc::generate_add_rpc_wit;
use descriptors::DescriptorIndex;
use error::Result;
use generator::generate_source;
use language::Language;
use spec::{ApiSpec, write_prepared_wit_directory};

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

pub struct AddRpcRequest {
    pub descriptor_path: PathBuf,
    pub rpc_name: String,
    pub input_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
}

pub struct DebugWitDirRequest {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
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

pub fn add_rpc_to_string(
    descriptor_path: impl AsRef<Path>,
    rpc_name: &str,
    input_path: Option<&Path>,
) -> Result<String> {
    let descriptors = DescriptorIndex::load(descriptor_path.as_ref())?;
    if let Some(input_path) = input_path {
        let input = fs::read_to_string(input_path).map_err(|source| error::Error::ReadFile {
            path: input_path.to_path_buf(),
            source,
        })?;
        add_rpc::generate_add_rpc_wit_with_input(&descriptors, rpc_name, input_path, &input)
    } else {
        generate_add_rpc_wit(&descriptors, rpc_name)
    }
}

pub fn add_rpc_to_file(request: &AddRpcRequest) -> Result<()> {
    let output = add_rpc_to_string(
        &request.descriptor_path,
        &request.rpc_name,
        request.input_path.as_deref(),
    )?;
    if let Some(path) = &request.output_path {
        fs::write(path, output).map_err(|source| error::Error::WriteFile {
            path: path.clone(),
            source,
        })?;
    } else {
        print!("{output}");
    }
    Ok(())
}

pub fn debug_wit_dir_to_file(request: &DebugWitDirRequest) -> Result<()> {
    write_prepared_wit_directory(&request.input_path, &request.output_path)
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

fn load_support_files(
    language: Language,
    spec: &ApiSpec,
    _input_path: &Path,
) -> Result<SupportFiles> {
    let support_contents = if spec.support.fragments.is_empty() {
        None
    } else {
        Some(
            spec.support
                .fragments
                .iter()
                .map(|fragment| fragment.contents.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };

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
