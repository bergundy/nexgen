//! The shared identifier case-mapping algorithm (P15).
//!
//! One algorithm maps a JSON member / type / operation name to each language's
//! idiomatic identifier: Go `PascalCase`, TypeScript/Java `camelCase`, Python
//! `snake_case`. The original JSON name is always pinned on the wire; this only
//! decides the in-code identifier. A `x-<lang>-name` override, when present,
//! wins verbatim. Collisions after mapping are the caller's problem (rejected at
//! load time).

use heck::{ToLowerCamelCase, ToPascalCase, ToSnakeCase};
use nex_gen_core::Language;

use crate::ir::NameOverrides;

/// The idiomatic member/field identifier for a JSON name in a language.
pub fn field_ident(json_name: &str, language: Language, overrides: &NameOverrides) -> String {
    if let Some(name) = override_for(language, overrides) {
        return name.to_string();
    }
    map_case(json_name, language)
}

/// The idiomatic type identifier (always PascalCase-family — types are
/// PascalCase in Go/Java/TS and stay PascalCase class names in Python too).
pub fn type_ident(name: &str, language: Language, overrides: &NameOverrides) -> String {
    if let Some(name) = override_for(language, overrides) {
        return name.to_string();
    }
    // Types are PascalCase in every target (Python classes included).
    let base = name.to_pascal_case();
    escape_keyword(&base, language)
}

/// The service-binding const/attribute identifier.
pub fn service_ident(name: &str, language: Language) -> String {
    match language {
        // Go: exported PascalCase var; TS: camelCase const; Python: PascalCase
        // class; Java: PascalCase interface.
        Language::Go | Language::Java => name.to_pascal_case(),
        Language::TypeScript => name.to_lower_camel_case(),
        Language::Python => name.to_pascal_case(),
        _ => name.to_pascal_case(),
    }
}

/// Map a name to a language's member-case convention (no override applied).
pub fn map_case(name: &str, language: Language) -> String {
    let mapped = match language {
        Language::Go => name.to_pascal_case(),
        Language::TypeScript | Language::Java => name.to_lower_camel_case(),
        Language::Python => name.to_snake_case(),
        _ => name.to_string(),
    };
    escape_keyword(&mapped, language)
}

fn override_for(language: Language, overrides: &NameOverrides) -> Option<&str> {
    match language {
        Language::Go => overrides.go.as_deref(),
        Language::Java => overrides.java.as_deref(),
        Language::Python => overrides.python.as_deref(),
        Language::TypeScript => overrides.typescript.as_deref(),
        _ => None,
    }
}

/// Suffix an underscore when a mapped identifier collides with a language
/// keyword. Kept minimal — the full reserved-word set matters only for exotic
/// inputs.
fn escape_keyword(name: &str, language: Language) -> String {
    let reserved = match language {
        Language::Python => PYTHON_KEYWORDS.contains(&name),
        Language::Go => GO_KEYWORDS.contains(&name),
        Language::Java => JAVA_KEYWORDS.contains(&name),
        Language::TypeScript => TS_KEYWORDS.contains(&name),
        _ => false,
    };
    if reserved {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue",
    "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import",
    "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
    "with", "yield", "match", "case",
];

const GO_KEYWORDS: &[&str] = &[
    "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for",
    "func", "go", "goto", "if", "import", "interface", "map", "package", "range", "return",
    "select", "struct", "switch", "type", "var",
];

const JAVA_KEYWORDS: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float",
    "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long", "native",
    "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp",
    "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void",
    "volatile", "while",
];

const TS_KEYWORDS: &[&str] = &[
    "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete", "do",
    "else", "enum", "export", "extends", "false", "finally", "for", "function", "if", "import",
    "in", "instanceof", "new", "null", "return", "super", "switch", "this", "throw", "true", "try",
    "typeof", "var", "void", "while", "with",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_member_cases() {
        assert_eq!(map_case("roomId", Language::Go), "RoomId");
        assert_eq!(map_case("roomId", Language::TypeScript), "roomId");
        assert_eq!(map_case("room_id", Language::TypeScript), "roomId");
        assert_eq!(map_case("roomId", Language::Python), "room_id");
        assert_eq!(map_case("displayName", Language::Python), "display_name");
    }

    #[test]
    fn escapes_python_keyword() {
        assert_eq!(map_case("class", Language::Python), "class_");
    }
}
