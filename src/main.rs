use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use nexus_api_gen::language::Language;
use nexus_api_gen::{
    AddRpcRequest, DebugWitDirRequest, GenerateRequest, add_rpc_to_file, debug_wit_dir_to_file,
    generate_to_file,
};

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
    #[command(
        about = "Add an RPC scaffold to an existing WIT file, or generate standalone WIT for one RPC"
    )]
    AddRpc(AddRpcArgs),
    #[command(about = "Write the prepared WIT workspace used for parsing to a directory")]
    DebugWitDir(DebugWitDirArgs),
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

#[derive(Args)]
struct AddRpcArgs {
    #[arg(long)]
    descriptors: PathBuf,
    #[arg(long)]
    rpc: String,
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Args)]
struct DebugWitDirArgs {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
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
        Commands::AddRpc(args) => add_rpc_to_file(&AddRpcRequest {
            descriptor_path: args.descriptors,
            rpc_name: args.rpc,
            input_path: args.input,
            output_path: args.output,
        }),
        Commands::DebugWitDir(args) => debug_wit_dir_to_file(&DebugWitDirRequest {
            input_path: args.input,
            output_path: args.output,
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
