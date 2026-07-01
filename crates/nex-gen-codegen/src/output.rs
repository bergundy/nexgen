//! Output plumbing: the generated-file model plus writing and formatting.
//!
//! Mirrors `GeneratedFiles` / `GeneratedOutputLayout` from the existing
//! crate's `src/generator.rs` and the `write_generated_files` /
//! `format_generated_file` / `formatter_command` logic from its `src/lib.rs`.
//! This layer is the same regardless of frontend, so it lives in the base.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};
use crate::language::Language;

/// Whether the output is a single file or a tree of files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneratedOutputLayout {
    SingleFile,
    Directory,
}

/// The full result of a generation run: a layout plus the rendered files
/// (keyed by output-relative path) and any non-fatal warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFiles {
    pub layout: GeneratedOutputLayout,
    pub files: BTreeMap<PathBuf, String>,
    pub warnings: Vec<String>,
}

impl GeneratedFiles {
    /// Build a single-file result from one rendered body.
    pub fn single_file(contents: String) -> Self {
        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("output"), contents);
        Self {
            layout: GeneratedOutputLayout::SingleFile,
            files,
            warnings: Vec::new(),
        }
    }

    /// Build a directory result from a map of output-relative paths to bodies.
    pub fn directory(files: BTreeMap<PathBuf, String>) -> Self {
        Self {
            layout: GeneratedOutputLayout::Directory,
            files,
            warnings: Vec::new(),
        }
    }

    /// The single file's contents, or `None` if this is a directory layout.
    pub fn single_file_contents(&self) -> Option<&str> {
        (self.layout == GeneratedOutputLayout::SingleFile)
            .then(|| self.files.values().next().map(String::as_str))
            .flatten()
    }
}

/// Write a [`GeneratedFiles`] result to `output_path`.
///
/// For [`GeneratedOutputLayout::SingleFile`] the single body is written to
/// `output_path` (creating parents). For [`GeneratedOutputLayout::Directory`]
/// the existing directory at `output_path` is replaced and each file written
/// under it. Mirrors `nex_gen::write_generated_files`.
pub fn write_generated_files(output_path: &Path, generated: &GeneratedFiles) -> Result<()> {
    match generated.layout {
        GeneratedOutputLayout::SingleFile => {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(|source| Error::WriteFile {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            fs::write(
                output_path,
                generated
                    .single_file_contents()
                    .expect("single-file output should contain one file"),
            )
            .map_err(|source| Error::WriteFile {
                path: output_path.to_path_buf(),
                source,
            })?;
        }
        GeneratedOutputLayout::Directory => {
            if output_path.is_file() {
                return Err(Error::OutputPathExists {
                    path: output_path.to_path_buf(),
                });
            }
            if output_path.exists() {
                fs::remove_dir_all(output_path).map_err(|source| Error::WriteFile {
                    path: output_path.to_path_buf(),
                    source,
                })?;
            }
            fs::create_dir_all(output_path).map_err(|source| Error::WriteFile {
                path: output_path.to_path_buf(),
                source,
            })?;

            for (relative_path, contents) in &generated.files {
                let path = output_path.join(relative_path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|source| Error::WriteFile {
                        path: parent.to_path_buf(),
                        source,
                    })?;
                }
                fs::write(&path, contents).map_err(|source| Error::WriteFile {
                    path: path.clone(),
                    source,
                })?;
            }
        }
    }

    Ok(())
}

/// Run the per-language formatter over the written output.
///
/// Mirrors `nex_gen::format_generated_file`.
pub fn format_generated_file(language: Language, output_path: &Path) -> Result<()> {
    let (program, args) = formatter_command(language, output_path)?;
    let command = format_formatter_command(program, &args);
    let status = Command::new(program)
        .args(&args)
        .status()
        .map_err(|source| Error::RunFormatter {
            path: output_path.to_path_buf(),
            command: command.clone(),
            source,
        })?;

    if !status.success() {
        return Err(Error::FormatterFailed {
            path: output_path.to_path_buf(),
            command,
            status,
        });
    }

    Ok(())
}

/// The formatter program and argument list for a language.
///
/// Mirrors `nex_gen::formatter_command`.
pub fn formatter_command(
    language: Language,
    output_path: &Path,
) -> Result<(&'static str, Vec<String>)> {
    let output_path = output_path.to_string_lossy().into_owned();
    match language {
        Language::Python => Ok((
            "ruff",
            vec![
                "format".to_string(),
                "--line-length".to_string(),
                "88".to_string(),
                output_path,
            ],
        )),
        Language::TypeScript => Ok((
            "prettier",
            vec![
                "--write".to_string(),
                "--print-width".to_string(),
                "88".to_string(),
                output_path,
            ],
        )),
        _ => Err(Error::UnsupportedLanguage { language }),
    }
}

fn format_formatter_command(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{format_formatter_command, formatter_command};
    use crate::language::Language;

    #[test]
    fn chooses_python_formatter_command() {
        let (program, args) = formatter_command(Language::Python, Path::new("output")).unwrap();
        assert_eq!(program, "ruff");
        assert_eq!(args, vec!["format", "--line-length", "88", "output"]);
        assert_eq!(
            format_formatter_command(program, &args),
            "ruff format --line-length 88 output"
        );
    }

    #[test]
    fn chooses_typescript_formatter_command() {
        let (program, args) =
            formatter_command(Language::TypeScript, Path::new("output")).unwrap();
        assert_eq!(program, "prettier");
        assert_eq!(args, vec!["--write", "--print-width", "88", "output"]);
        assert_eq!(
            format_formatter_command(program, &args),
            "prettier --write --print-width 88 output"
        );
    }
}
