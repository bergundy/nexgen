//! WIT front-end loader lowering WIT inputs into the shared core IR.
//!
//! This is the WIT side of the `Loader` -> `IR` -> `Emitter` pipeline described
//! in `crates/nex-gen-core`. [`WitLoader`] validates its inputs and lowers
//! them into a [`SymbolTable<WitSymbolKind>`] via
//! [`build_wit_symbols`](crate::wit_symbols::build_wit_symbols) — that table *is* the WIT
//! IR. The symbol machinery (`WitSymbolKind`, the
//! table build, and the [`WitSymbols`](crate::wit_symbols::WitSymbols) view the
//! emitters read) lives in `crate::wit_symbols`.

use std::path::PathBuf;

use nex_gen_core::{IR, Language, LoadOutput};

use crate::descriptors::DescriptorIndex;
use crate::generator::generation_warnings;
use crate::resources::ensure_unique_resource_names;
use crate::spec::ApiSpec;
use crate::validation::validate_type_overrides;
use crate::wit_symbols::{WitSymbolKind, WitSymbols, build_wit_symbols};

/// Loads WIT inputs (plus proto descriptors and support files) into the core IR.
///
/// Holds its own inputs; `language` is supplied per [`Loader::load`] call
/// because WIT resolves language-specific overrides at parse time — and support
/// fragments are picked per-language too. Support files are a loader *input*:
/// the loader resolves them (spec-embedded plus external `--support` paths) and
/// lowers them into [`Fragment`](WitSymbolKind::Fragment) symbols, so the single
/// spec parse here feeds both the type symbols and the fragment symbols.
pub(crate) struct WitLoader {
    input_paths: Vec<PathBuf>,
    descriptor_paths: Vec<PathBuf>,
    support_paths: Vec<PathBuf>,
}

impl WitLoader {
    /// Construct a loader over the given WIT input, proto descriptor, and
    /// external support-file paths.
    pub(crate) fn new(
        input_paths: Vec<PathBuf>,
        descriptor_paths: Vec<PathBuf>,
        support_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            input_paths,
            descriptor_paths,
            support_paths,
        }
    }
}

impl nex_gen_core::Loader for WitLoader {
    type Kind = WitSymbolKind;

    fn load(&self, language: Language) -> nex_gen_core::Result<LoadOutput<WitSymbolKind>> {
        let spec = ApiSpec::load_for_language_with_inputs(language, &self.input_paths).map_err(
            |error| nex_gen_core::Error::Load {
                message: error.to_string(),
            },
        )?;
        let descriptors = DescriptorIndex::load_many(&self.descriptor_paths).map_err(|error| {
            nex_gen_core::Error::Load {
                message: error.to_string(),
            }
        })?;
        validate_type_overrides(&spec, &descriptors, language).map_err(|error| {
            nex_gen_core::Error::Load {
                message: error.to_string(),
            }
        })?;
        ensure_unique_resource_names(&spec).map_err(|error| nex_gen_core::Error::Load {
            message: error.to_string(),
        })?;
        let support =
            crate::load_support_files(language, &spec, &self.support_paths).map_err(|error| {
                nex_gen_core::Error::Load {
                    message: error.to_string(),
                }
            })?;
        let symbols =
            build_wit_symbols(&spec, &descriptors, &support.fragments).map_err(|error| {
                nex_gen_core::Error::Load {
                    message: error.to_string(),
                }
            })?;
        let warnings = generation_warnings(&WitSymbols::new(&symbols));
        Ok(LoadOutput::with_warnings(IR::new(symbols), warnings))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use prost_types::FileDescriptorSet;

    use crate::Language;
    use crate::descriptors::DescriptorIndex;
    use crate::spec::{ApiSpec, SupportFragmentSpec};
    use crate::wit_symbols::{WitSymbolKind, build_wit_symbols};

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

    fn build_symbols() -> nex_gen_core::SymbolTable<WitSymbolKind> {
        let spec =
            ApiSpec::parse_for_language(Language::Python, INLINE_WIT, PathBuf::from("inline.wit"))
                .unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        build_wit_symbols(&spec, &descriptors, &[]).unwrap()
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
    fn lowers_support_fragments_into_symbols() {
        let spec =
            ApiSpec::parse_for_language(Language::Python, INLINE_WIT, PathBuf::from("inline.wit"))
                .unwrap();
        let descriptors =
            DescriptorIndex::from_descriptor_set(FileDescriptorSet { file: Vec::new() }).unwrap();
        let fragments = vec![SupportFragmentSpec {
            path: "support/helpers.py".to_string(),
            contents: "def helper():\n    pass\n".to_string(),
            namespace: None,
        }];
        let table = build_wit_symbols(&spec, &descriptors, &fragments).unwrap();

        let fragment_symbol = table
            .iter()
            .find(|symbol| matches!(&symbol.kind, WitSymbolKind::Fragment(_)))
            .expect("a fragment symbol should exist");
        let WitSymbolKind::Fragment(fragment) = &fragment_symbol.kind else {
            unreachable!()
        };
        assert_eq!(fragment_symbol.name.as_str(), "support/helpers.py");
        assert_eq!(fragment.contents, "def helper():\n    pass\n");
        assert!(
            fragment_symbol.refs.is_empty(),
            "fragment symbols are opaque and carry no refs"
        );
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
        let input_id = operation.input;

        // A service references its operations' input/output symbols directly.
        assert!(
            service_symbol.refs.contains(&input_id),
            "service refs {:?} should include input symbol id {:?}",
            service_symbol.refs,
            input_id
        );

        // The input symbol, in turn, references its backing model symbol.
        let input_symbol = table
            .get(input_id)
            .expect("operation input symbol should exist");
        let WitSymbolKind::OperationInput(input) = &input_symbol.kind else {
            panic!("operation input id should resolve to an OperationInput symbol");
        };
        let input_model_name = &input.model_name;
        let input_model_symbol = table
            .iter()
            .find(|symbol| {
                matches!(&symbol.kind, WitSymbolKind::Model(model) if &model.name == input_model_name)
            })
            .expect("operation input model should be a symbol");

        assert!(
            input_symbol.refs.contains(&input_model_symbol.id),
            "input symbol refs {:?} should include input model id {:?}",
            input_symbol.refs,
            input_model_symbol.id
        );
    }
}
