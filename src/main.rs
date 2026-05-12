use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use nexus_api_gen::language::Language;
use nexus_api_gen::{GenerateRequest, generate_to_file};

#[derive(Parser)]
#[command(name = "nexus-api-gen")]
#[command(
    about = "Generate language-specific Nexus operation bindings from WIT and protobuf descriptors"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate(GenerateArgs),
}

#[derive(Args)]
struct GenerateArgs {
    #[arg(long, value_enum)]
    lang: CliLanguage,
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    descriptors: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    format: bool,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum CliLanguage {
    Python,
    Typescript,
}

impl From<CliLanguage> for Language {
    fn from(value: CliLanguage) -> Self {
        match value {
            CliLanguage::Python => Language::Python,
            CliLanguage::Typescript => Language::TypeScript,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Generate(args) => generate_to_file(&GenerateRequest {
            language: args.lang.into(),
            input_path: args.input,
            descriptor_path: args.descriptors,
            output_path: args.output,
            format: args.format,
        }),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
