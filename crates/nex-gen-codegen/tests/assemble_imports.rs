//! End-to-end import-resolution + assembly test.
//!
//! Hand-builds a small [`SymbolTable`] — a service plus two types in a `models`
//! module and one foreign (proto-style) type — and a trivial test
//! [`Emitter`]/[`NameResolver`], then runs [`assemble`] and asserts the
//! base-owned resolution rules: cross-module refs import, same-module refs do
//! not, the foreign ref produces a namespace/foreign import, imports dedup, and
//! the service file imports its I/O types.
//!
//! The test emitter renders a minimal inline service body (it does NOT depend
//! on the real `render_service`, which lands in Phase 2).

use std::collections::BTreeMap;
use std::path::PathBuf;

use nex_gen_codegen::emit::{EmittedFile, Import, ImportBinding, Module};
use nex_gen_codegen::ir::{
    Name, Operation, ServiceDef, Symbol, SymbolId, SymbolKind, SymbolTable,
};
use nex_gen_codegen::{
    GeneratedOutputLayout, Language, NameResolver, SchemaType, assemble, render_imports,
    traits::Emitter,
};

const MODELS_MODULE: &str = "./models.ts";
const SERVICE_MODULE: &str = "./service.ts";
const PROTO_MODULE: &str = "@temporalio/proto";

/// Build: a `UserService` (in `service.ts`) with one operation taking
/// `GetUserRequest` (models) and returning `User` (a foreign proto type), plus
/// a second `models` type `Extra` referenced by `GetUserRequest` (same-module
/// from the models file's perspective).
fn build_table() -> (SymbolTable, SymbolId, SymbolId, SymbolId, SymbolId) {
    let mut table = SymbolTable::new();

    let req = table.alloc_id();
    let extra = table.alloc_id();
    let user = table.alloc_id();
    let svc = table.alloc_id();

    // models: GetUserRequest references Extra (same module => no import).
    table.insert(Symbol {
        id: req,
        name: Name::new("GetUserRequest"),
        kind: SymbolKind::Type,
        refs: vec![extra],
    });
    table.insert(Symbol {
        id: extra,
        name: Name::new("Extra"),
        kind: SymbolKind::Type,
        refs: vec![],
    });
    // foreign proto type (its module is the foreign package; empty body).
    table.insert(Symbol {
        id: user,
        name: Name::new("User"),
        kind: SymbolKind::Type,
        refs: vec![],
    });
    // service: operation in I/O -> req (models), out -> user (foreign).
    table.insert(Symbol {
        id: svc,
        name: Name::new("UserService"),
        kind: SymbolKind::Service(ServiceDef {
            name: Name::new("UserService"),
            wire_name: "UserService".to_string(),
            operations: vec![Operation {
                name: Name::new("GetUser"),
                wire_name: "GetUser".to_string(),
                input: Some(req),
                output: Some(user),
                docs: None,
            }],
            docs: None,
        }),
        refs: vec![req, user],
    });

    (table, req, extra, user, svc)
}

struct TestResolver {
    req: SymbolId,
    extra: SymbolId,
    user: SymbolId,
}

impl NameResolver for TestResolver {
    fn type_ref(&self, id: SymbolId) -> String {
        match id {
            i if i == self.req => "GetUserRequest".to_string(),
            i if i == self.extra => "Extra".to_string(),
            i if i == self.user => "temporal.api.user.v1.User".to_string(),
            _ => panic!("unknown symbol {id:?}"),
        }
    }

    fn module_of(&self, id: SymbolId) -> Module {
        match id {
            i if i == self.req || i == self.extra => Module::new(MODELS_MODULE),
            i if i == self.user => Module::new(PROTO_MODULE),
            _ => panic!("unknown symbol {id:?}"),
        }
    }

    fn import_binding(&self, id: SymbolId) -> Import {
        match id {
            i if i == self.req => Import {
                module: Module::new(MODELS_MODULE),
                name: Some("GetUserRequest".to_string()),
                binding: ImportBinding::Named,
                type_only: true,
            },
            i if i == self.extra => Import {
                module: Module::new(MODELS_MODULE),
                name: Some("Extra".to_string()),
                binding: ImportBinding::Named,
                type_only: true,
            },
            // foreign proto: namespace-head import (`{ temporal }`).
            i if i == self.user => Import {
                module: Module::new(PROTO_MODULE),
                name: Some("temporal".to_string()),
                binding: ImportBinding::Named,
                type_only: true,
            },
            _ => panic!("unknown symbol {id:?}"),
        }
    }
}

struct TestEmitter {
    resolver: TestResolver,
    req: SymbolId,
    extra: SymbolId,
    user: SymbolId,
}

impl Emitter for TestEmitter {
    fn language(&self) -> Language {
        Language::TypeScript
    }

    fn schema_type(&self) -> SchemaType {
        SchemaType::Wit
    }

    fn emit(&self, symbols: &SymbolTable) -> Vec<EmittedFile> {
        // models.ts: the two model types. GetUserRequest references Extra
        // (same module) and Extra references nothing -> no cross-module refs.
        let models = EmittedFile {
            path: PathBuf::from("models.ts"),
            module: Module::new(MODELS_MODULE),
            refs: vec![self.req, self.extra],
            runtime_imports: vec![],
            body: "export interface GetUserRequest { extra: Extra }\n\
                   export interface Extra {}"
                .to_string(),
        };

        // service.ts: inline-rendered service body (NOT via render_service).
        // It references its I/O types: GetUserRequest (models) + User (proto).
        let mut body = String::new();
        for symbol in symbols.iter() {
            if let SymbolKind::Service(def) = &symbol.kind {
                body.push_str(&format!("export const {} = {{\n", def.name.as_str()));
                for op in &def.operations {
                    let input = op
                        .input
                        .map(|id| self.resolver.type_ref(id))
                        .unwrap_or_else(|| "void".to_string());
                    let output = op
                        .output
                        .map(|id| self.resolver.type_ref(id))
                        .unwrap_or_else(|| "void".to_string());
                    body.push_str(&format!(
                        "  {}: op<{input}, {output}>(),\n",
                        op.name.as_str()
                    ));
                }
                body.push_str("};");
            }
        }
        let service = EmittedFile {
            path: PathBuf::from("service.ts"),
            module: Module::new(SERVICE_MODULE),
            refs: vec![self.req, self.user],
            // runtime import that every service file needs (non-symbol).
            runtime_imports: vec![Import {
                module: Module::new("nexus-rpc"),
                name: Some("nexus".to_string()),
                binding: ImportBinding::Module,
                type_only: false,
            }],
            body,
        };

        vec![models, service]
    }

    fn resolver(&self) -> &dyn NameResolver {
        &self.resolver
    }
}

#[test]
fn assemble_resolves_cross_module_and_foreign_imports() {
    let (table, req, extra, user, _svc) = build_table();
    let emitter = TestEmitter {
        resolver: TestResolver { req, extra, user },
        req,
        extra,
        user,
    };

    let generated = assemble(&table, &emitter).expect("assemble");

    // Two distinct paths -> directory layout (driven by emitter output, not a
    // content guess).
    assert_eq!(generated.layout, GeneratedOutputLayout::Directory);
    let files: BTreeMap<_, _> = generated.files.clone();
    let models = files
        .get(&PathBuf::from("models.ts"))
        .expect("models.ts present");
    let service = files
        .get(&PathBuf::from("service.ts"))
        .expect("service.ts present");

    // models.ts: both its refs (GetUserRequest, Extra) are same-module, so it
    // must have NO import block at all.
    assert!(
        !models.contains("import"),
        "models.ts should not import same-module refs, got:\n{models}"
    );

    // service.ts: imports its I/O types. GetUserRequest is a cross-module named
    // import from ./models.ts; the proto output is a foreign namespace-head
    // import from @temporalio/proto; the runtime nexus import is present.
    assert!(
        service.contains("import type { GetUserRequest } from \"./models.ts\";"),
        "service.ts should import its input type, got:\n{service}"
    );
    assert!(
        service.contains("import type { temporal } from \"@temporalio/proto\";"),
        "service.ts should import the foreign proto namespace head, got:\n{service}"
    );
    assert!(
        service.contains("import * as nexus from \"nexus-rpc\";"),
        "service.ts should keep its non-symbol runtime import, got:\n{service}"
    );
    // The service body (stitched after the import block) names the I/O types.
    assert!(service.contains("GetUserRequest"));
    assert!(service.contains("temporal.api.user.v1.User"));
}

#[test]
fn render_imports_dedups_and_merges_named() {
    // Two refs to the same module + a duplicate -> one merged, deduped Named
    // statement.
    let imports = vec![
        Import {
            module: Module::new("./models.ts"),
            name: Some("B".to_string()),
            binding: ImportBinding::Named,
            type_only: true,
        },
        Import {
            module: Module::new("./models.ts"),
            name: Some("A".to_string()),
            binding: ImportBinding::Named,
            type_only: true,
        },
        // duplicate of the first
        Import {
            module: Module::new("./models.ts"),
            name: Some("B".to_string()),
            binding: ImportBinding::Named,
            type_only: true,
        },
    ];
    let block = render_imports(Language::TypeScript, &imports);
    assert_eq!(block, "import type { A, B } from \"./models.ts\";");

    // Python named import of >1 name wraps in parens.
    let block = render_imports(Language::Python, &imports);
    assert_eq!(block, "from ./models.ts import (\n    A,\n    B,\n)");
}
