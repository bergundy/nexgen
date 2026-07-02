//! Per-language `render_imports` shaping tests.
//!
//! Rendering is per-language: each `render::<lang>` module owns its minimal
//! `Import` type and renders its own import block. These tests pin the shaping
//! rules — dedup/merge, type-only handling, and `using` order.

use nex_gen_core::render::{dotnet, python, typescript};

#[test]
fn typescript_dedups_and_merges_named() {
    // Two named refs to the same module + a duplicate -> one merged, deduped,
    // sorted statement.
    let imports = vec![
        typescript::Import::Named {
            module: "./models.ts".to_string(),
            name: "B".to_string(),
            type_only: true,
        },
        typescript::Import::Named {
            module: "./models.ts".to_string(),
            name: "A".to_string(),
            type_only: true,
        },
        typescript::Import::Named {
            module: "./models.ts".to_string(),
            name: "B".to_string(),
            type_only: true,
        },
    ];
    assert_eq!(
        typescript::render_imports(&imports),
        "import type { A, B } from \"./models.ts\";"
    );
}

#[test]
fn typescript_keeps_type_only_and_value_separate() {
    // Type-only and value imports for the same module cannot merge.
    let imports = vec![
        typescript::Import::Named {
            module: "./models.ts".to_string(),
            name: "A".to_string(),
            type_only: true,
        },
        typescript::Import::Named {
            module: "./models.ts".to_string(),
            name: "B".to_string(),
            type_only: false,
        },
        typescript::Import::Star {
            module: "nexus-rpc".to_string(),
            alias: "nexus".to_string(),
            type_only: false,
        },
    ];
    // Star imports first (sorted), then Named grouped per (module, type_only) —
    // value imports (`type_only = false`) before type-only for the same module.
    assert_eq!(
        typescript::render_imports(&imports),
        "import * as nexus from \"nexus-rpc\";\n\
         import { B } from \"./models.ts\";\n\
         import type { A } from \"./models.ts\";"
    );
}

#[test]
fn python_merges_named_and_emits_module_imports() {
    let imports = vec![
        python::Import::Named {
            module: "./models".to_string(),
            name: "B".to_string(),
        },
        python::Import::Named {
            module: "./models".to_string(),
            name: "A".to_string(),
        },
        python::Import::Module {
            module: "pkg.common".to_string(),
        },
    ];
    // Multi-name `from` import wraps in parens; whole-module `import` lines come
    // first (module_imports render ahead of named).
    assert_eq!(
        python::render_imports(&imports),
        "import pkg.common\nfrom ./models import (\n    A,\n    B,\n)"
    );
}

#[test]
fn dotnet_emits_using_lines_in_order() {
    // .NET imports are whole-namespace `using X;`, rendered in the given order
    // (not sorted, not deduped — the caller supplies dependency order).
    let imports = vec![
        dotnet::Import {
            module: "System".to_string(),
        },
        dotnet::Import {
            module: "System.CodeDom.Compiler".to_string(),
        },
        dotnet::Import {
            module: "NexusRpc".to_string(),
        },
    ];
    assert_eq!(
        dotnet::render_imports(&imports),
        "using System;\nusing System.CodeDom.Compiler;\nusing NexusRpc;"
    );

    // No imports -> empty block.
    assert_eq!(dotnet::render_imports(&[]), "");
}
