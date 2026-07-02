//! The `nex-gen` CLI — a thin parser over the in-process API (P16).
//!
//! ```text
//! nex-gen --lang LANG [--out-dir DIR | --out-file FILE] [--dry-run] SCHEMA_FILE|DIR ...
//! ```

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use nex_gen_core::{Language, format_generated_file, write_generated_files};

/// Generate model code and validators from JSON Schema 2020-12 definition
/// files, including Nexus service bindings declared in a Nexus document.
#[derive(Parser, Debug)]
#[command(name = "nex-gen", about, long_about = None)]
struct Cli {
    /// The target language.
    #[arg(long, value_enum)]
    lang: LangArg,

    /// Output directory. Mutually exclusive with --out-file.
    #[arg(long, conflicts_with = "out_file")]
    out_dir: Option<PathBuf>,

    /// Output file (single-file targets only). Mutually exclusive with --out-dir.
    #[arg(long)]
    out_file: Option<PathBuf>,

    /// Print every file that would be written to stdout instead of writing it.
    #[arg(long)]
    dry_run: bool,

    /// One or more schema files (or directories) to generate from.
    #[arg(required = true)]
    schema: Vec<PathBuf>,
}

/// The `--lang` CLI values (`go | java | py | ts`).
#[derive(Copy, Clone, Debug, ValueEnum)]
enum LangArg {
    Go,
    Java,
    Py,
    Ts,
}

impl From<LangArg> for Language {
    fn from(value: LangArg) -> Self {
        match value {
            LangArg::Go => Language::Go,
            LangArg::Java => Language::Java,
            LangArg::Py => Language::Python,
            LangArg::Ts => Language::TypeScript,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let language: Language = cli.lang.into();
    let inputs = collect_inputs(&cli.schema)?;
    if inputs.is_empty() {
        return Err("no schema files found".to_string());
    }

    let generated =
        nex_gen_json_schema::generate(inputs, language).map_err(|e| e.to_string())?;

    if cli.dry_run {
        for (path, body) in &generated.files {
            println!("// ===== {} =====", path.display());
            print!("{body}");
            if !body.ends_with('\n') {
                println!();
            }
        }
        return Ok(());
    }

    match (&cli.out_dir, &cli.out_file) {
        (Some(dir), None) => {
            write_generated_files(dir, &generated).map_err(|e| e.to_string())?;
            if matches!(language, Language::Python | Language::TypeScript) {
                let _ = format_generated_file(language, dir);
            }
        }
        (None, Some(file)) => {
            if generated.files.len() != 1 {
                return Err(format!(
                    "--out-file needs a single-file target, but {language} emits {} files; use --out-dir",
                    generated.files.len()
                ));
            }
            let body = generated.files.values().next().expect("one file");
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(file, body).map_err(|e| e.to_string())?;
        }
        (None, None) => return Err("one of --out-dir or --out-file is required".to_string()),
        (Some(_), Some(_)) => unreachable!("clap enforces mutual exclusion"),
    }

    Ok(())
}

/// Expand directory inputs into their `*.yaml` / `*.yml` / `*.json` files.
fn collect_inputs(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut inputs = Vec::new();
    for path in paths {
        if path.is_dir() {
            let entries =
                std::fs::read_dir(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let mut files: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .filter(|p| {
                    matches!(
                        p.extension().and_then(|e| e.to_str()),
                        Some("yaml") | Some("yml") | Some("json")
                    )
                })
                .collect();
            files.sort();
            inputs.extend(files);
        } else {
            inputs.push(path.clone());
        }
    }
    Ok(inputs)
}
