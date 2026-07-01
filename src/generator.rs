use crate::SupportFiles;
use crate::wit_symbols::{WitSymbols, build_wit_symbols};
use crate::descriptors::DescriptorIndex;
use crate::dotnet;
use crate::error::{Error, Result};
use crate::Language;
use crate::python;
use crate::resources::ensure_unique_resource_names;
use crate::spec::ApiSpec;
use crate::typescript;
use crate::validation::validate_type_overrides;

// The generated-file model + layout now live in the base crate (the output
// plumbing is frontend-agnostic). Re-exported here so existing
// `crate::generator::GeneratedFiles` paths keep working.
pub use nex_gen_codegen::{GeneratedFiles, GeneratedOutputLayout};

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
    let support_fragments = if support.fragments.is_empty() {
        spec.support.fragments_for_language(language)
    } else {
        &support.fragments
    };
    let table = build_wit_symbols(spec, descriptors, support_fragments)?;
    let symbols = WitSymbols::new(&table);
    let warnings = generation_warnings(&symbols);

    let mut generated = match language {
        Language::Dotnet => dotnet::generate(&symbols),
        Language::Python => python::generate(&symbols),
        Language::TypeScript => typescript::generate(&symbols),
        language => Err(Error::UnsupportedLanguage { language }),
    }?;
    generated.warnings = warnings;
    Ok(generated)
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

pub(crate) fn generation_warnings(symbols: &WitSymbols) -> Vec<String> {
    symbols.services()
        .flat_map(|service| {
            service.resources.iter().flat_map(|resource| {
                resource.methods.iter().filter_map(|method| {
                    matches!(
                        method.binding,
                        crate::wit_symbols::WitResourceMethodBindingSpec::Stub
                    )
                    .then(|| {
                        format!(
                            "resource method `{}.{}` generated as a stub because no operation could be bound",
                            resource.type_name, method.name
                        )
                    })
                })
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use prost_types::FileDescriptorSet;

    use crate::SupportFiles;
    use crate::descriptors::DescriptorIndex;
    use crate::Language;
    use crate::spec::ApiSpec;

    use super::generate_files;

    #[test]
    fn warns_when_resource_method_generates_as_stub() {
        let wit = r#"
package temporal:users@1.0.0;

world system {
  export user-service;
}

/// @nexus.endpoint "__user_service"
interface user-service {
  resource user {
    constructor(user-id: string, email: string);

    update-email: func(email: string) -> user-result;
  }

  type user-result = own<user>;

  record update-email-request {
    users-id: string,
    email: string,
  }

  update-email: func(request: update-email-request) -> user-result;
}
"#;
        let spec = ApiSpec::parse_for_language(Language::Python, wit, PathBuf::from("inline.wit"))
            .unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        let generated = generate_files(
            Language::Python,
            &spec,
            &descriptors,
            &SupportFiles::default(),
        )
        .unwrap();

        assert_eq!(
            generated.warnings,
            vec![
                "resource method `User.update-email` generated as a stub because no operation could be bound"
                    .to_string()
            ]
        );
    }
}
