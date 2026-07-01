//! WIT front-end loader lowering WIT inputs into the shared base IR.
//!
//! This is the WIT side of the `Loader` -> `IR` -> `Emitter` pipeline described
//! in `crates/nex-gen-codegen`. [`WitLoader`] validates its inputs and lowers
//! them into a [`SymbolTable<WitSymbolKind>`] via
//! [`build_api_plan`](crate::api_plan::build_api_plan) — that table *is* the WIT
//! IR (there is no `ApiPlan`). The symbol machinery (`WitSymbolKind`, the
//! table build, and the [`WitSymbols`](crate::api_plan::WitSymbols) view the
//! emitters read) lives in `crate::api_plan`.

use std::path::PathBuf;

use nex_gen_codegen::{IR, Language, SchemaType};

use crate::api_plan::{WitSymbolKind, build_api_plan};
use crate::descriptors::DescriptorIndex;
use crate::resources::ensure_unique_resource_names;
use crate::spec::ApiSpec;
use crate::validation::validate_type_overrides;

/// Loads WIT inputs (plus proto descriptors) into the base IR.
///
/// Holds its own inputs; `language` is supplied per [`Loader::load`] call
/// because WIT resolves language-specific overrides at parse time.
pub(crate) struct WitLoader {
    input_paths: Vec<PathBuf>,
    descriptor_paths: Vec<PathBuf>,
}

impl WitLoader {
    /// Construct a loader over the given WIT input and proto descriptor paths.
    pub(crate) fn new(input_paths: Vec<PathBuf>, descriptor_paths: Vec<PathBuf>) -> Self {
        Self {
            input_paths,
            descriptor_paths,
        }
    }
}

impl nex_gen_codegen::Loader for WitLoader {
    type Kind = WitSymbolKind;

    fn schema_type(&self) -> SchemaType {
        SchemaType::Wit
    }

    fn load(&self, language: Language) -> nex_gen_codegen::Result<IR<WitSymbolKind>> {
        let spec = ApiSpec::load_for_language_with_inputs(language, &self.input_paths)
            .map_err(|error| nex_gen_codegen::Error::Load {
                message: error.to_string(),
            })?;
        let descriptors = DescriptorIndex::load_many(&self.descriptor_paths).map_err(|error| {
            nex_gen_codegen::Error::Load {
                message: error.to_string(),
            }
        })?;
        validate_type_overrides(&spec, &descriptors, language).map_err(|error| {
            nex_gen_codegen::Error::Load {
                message: error.to_string(),
            }
        })?;
        ensure_unique_resource_names(&spec).map_err(|error| nex_gen_codegen::Error::Load {
            message: error.to_string(),
        })?;
        let symbols = build_api_plan(&spec, &descriptors).map_err(|error| {
            nex_gen_codegen::Error::Load {
                message: error.to_string(),
            }
        })?;
        Ok(IR::new(symbols))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use prost_types::FileDescriptorSet;

    use crate::Language;
    use crate::api_plan::{WitSymbolKind, build_api_plan};
    use crate::descriptors::DescriptorIndex;
    use crate::spec::ApiSpec;

    const INLINE_WIT: &str = r#"
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

    fn build_symbols() -> nex_gen_codegen::SymbolTable<WitSymbolKind> {
        let spec =
            ApiSpec::parse_for_language(Language::Python, INLINE_WIT, PathBuf::from("inline.wit"))
                .unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        build_api_plan(&spec, &descriptors).unwrap()
    }

    #[test]
    fn explodes_single_service_symbol() {
        let table = build_symbols();
        let services: Vec<&str> = table
            .iter()
            .filter_map(|symbol| match &symbol.kind {
                WitSymbolKind::Service(service) => Some(service.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(services, vec!["UserService"]);
    }

    #[test]
    fn includes_model_symbols_by_name() {
        let table = build_symbols();
        let has_model = table.iter().any(|symbol| {
            matches!(&symbol.kind, WitSymbolKind::Model(model) if model.name == "UpdateEmailRequest")
        });
        assert!(
            has_model,
            "expected an UpdateEmailRequest model symbol to be present"
        );
    }

    #[test]
    fn service_refs_include_input_model() {
        let table = build_symbols();

        // Find the sole service symbol and the full_name of its first
        // operation's input model.
        let service_symbol = table
            .iter()
            .find(|symbol| matches!(&symbol.kind, WitSymbolKind::Service(_)))
            .expect("a service symbol should exist");
        let WitSymbolKind::Service(service) = &service_symbol.kind else {
            unreachable!()
        };
        let operation = service
            .operations
            .first()
            .expect("service should have at least one operation");
        let input_model_name = &operation.input.model_name;

        // Find the model symbol matching the operation input's model name.
        let input_model_symbol = table
            .iter()
            .find(|symbol| {
                matches!(&symbol.kind, WitSymbolKind::Model(model) if &model.name == input_model_name)
            })
            .expect("operation input model should be a symbol");

        assert!(
            service_symbol.refs.contains(&input_model_symbol.id),
            "service refs {:?} should include input model id {:?}",
            service_symbol.refs,
            input_model_symbol.id
        );
    }
}
