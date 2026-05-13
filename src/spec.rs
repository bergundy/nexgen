use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use heck::{ToSnakeCase, ToUpperCamelCase};
use tempfile::TempDir;
use wit_parser::{
    Function, FunctionKind, Handle, Interface, PackageId, PackageSourceMap, Resolve, Type, TypeDef,
    TypeDefKind, WorldItem, WorldKey,
};

use crate::error::{Error, Result};
use crate::language::Language;

type PackageOrigins = BTreeMap<PackageId, PathBuf>;

pub(crate) struct ParsedWitPackage {
    pub resolve: Resolve,
    pub package_id: PackageId,
    pub package_origins: PackageOrigins,
    _workspace: TempDir,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiSpec {
    pub version: String,
    pub support: SupportSpec,
    pub services: Vec<ServiceSpec>,
    pub types: BTreeMap<String, TypeOverrideSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinTypeMetadata {
    pub wit_name: String,
    pub use_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinWitMetadata {
    pub proto_types: BTreeMap<String, BuiltinTypeMetadata>,
    pub type_use_paths: BTreeMap<String, String>,
}

impl ApiSpec {
    pub fn load_for_language(language: Language, path: &Path) -> Result<Self> {
        let input = fs::read_to_string(path).map_err(|source| Error::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse_for_language(language, &input, path.to_path_buf())
    }

    pub fn parse_for_language(language: Language, input: &str, path: PathBuf) -> Result<Self> {
        let parsed = parse_wit_with_builtins(input, &path)?;
        Self::from_wit(
            language,
            &parsed.resolve,
            parsed.package_id,
            &parsed.package_origins,
            path,
        )
    }

    pub fn type_override(&self, type_name: &str) -> Option<&TypeOverrideSpec> {
        self.types.get(type_name.trim_start_matches('.'))
    }

    fn from_wit(
        language: Language,
        resolve: &Resolve,
        package_id: PackageId,
        package_origins: &PackageOrigins,
        path: PathBuf,
    ) -> Result<Self> {
        let package = &resolve.packages[package_id];
        let world_id = select_world(resolve, package_id, &path)?;
        let world = &resolve.worlds[world_id];
        let support = collect_support_spec(language, resolve, package_id, package_origins)?;

        let mut types = BTreeMap::new();
        for (_, dependency_package) in resolve.packages.iter() {
            for interface_id in dependency_package.interfaces.values() {
                let interface = &resolve.interfaces[*interface_id];
                collect_interface_types(language, resolve, interface, &path, &mut types)?;
            }
        }

        let mut services = Vec::new();
        for (key, item) in &world.exports {
            let WorldItem::Interface { id, .. } = item else {
                continue;
            };
            let interface = &resolve.interfaces[*id];
            services.push(build_service(language, resolve, key, interface, &path)?);
        }

        Ok(Self {
            version: package
                .name
                .version
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "0.0.0".to_string()),
            support,
            services,
            types,
        })
    }
}

pub fn write_prepared_wit_directory(input_path: &Path, output_path: &Path) -> Result<()> {
    if output_path.exists() {
        return Err(Error::OutputPathExists {
            path: output_path.to_path_buf(),
        });
    }

    let input = fs::read_to_string(input_path).map_err(|source| Error::ReadFile {
        path: input_path.to_path_buf(),
        source,
    })?;
    let workspace = prepare_wit_workspace(&input, input_path)?;
    copy_directory_tree(&workspace.package_root, output_path)?;
    Ok(())
}

pub(crate) fn parse_wit_with_builtins(input: &str, path: &Path) -> Result<ParsedWitPackage> {
    let workspace = prepare_wit_workspace(input, path)?;
    let mut resolve = Resolve::default();
    let (package_id, source_map) =
        resolve
            .push_dir(&workspace.package_root)
            .map_err(|error| Error::WitParse {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
    let package_origins = collect_package_origins(&resolve, &source_map)?;
    Ok(ParsedWitPackage {
        resolve,
        package_id,
        package_origins,
        _workspace: workspace.temp_dir,
    })
}

pub fn load_builtin_wit_metadata() -> Result<BuiltinWitMetadata> {
    let workspace = prepare_builtin_metadata_workspace()?;
    let mut resolve = Resolve::default();
    let (main_package_id, source_map) =
        resolve
            .push_dir(&workspace.package_root)
            .map_err(|error| Error::InvalidWit {
                path: repo_builtins_root(),
                reason: format!("failed to parse bundled built-in WIT: {error}"),
            })?;
    let package_origins = collect_package_origins(&resolve, &source_map)?;

    let mut proto_types = BTreeMap::new();
    let mut type_use_paths = BTreeMap::new();

    for (package_id, package) in resolve.packages.iter() {
        if package_id == main_package_id {
            continue;
        }

        let package_name = if let Some(version) = &package.name.version {
            format!(
                "{}:{}@{}",
                package.name.namespace, package.name.name, version
            )
        } else {
            format!("{}:{}", package.name.namespace, package.name.name)
        };
        let origin_path = package_origins
            .get(&package_id)
            .cloned()
            .unwrap_or_else(|| repo_builtins_root());

        for interface_id in package.interfaces.values() {
            let interface = &resolve.interfaces[*interface_id];
            let Some(interface_name) = interface.name.as_deref() else {
                continue;
            };
            let use_path = if let Some(version) = &package.name.version {
                format!(
                    "{}:{}/{}@{}",
                    package.name.namespace, package.name.name, interface_name, version
                )
            } else {
                format!(
                    "{}:{}/{}",
                    package.name.namespace, package.name.name, interface_name
                )
            };

            for type_id in interface.types.values() {
                let type_def = &resolve.types[*type_id];
                let Some(type_name) = type_def.name.as_deref() else {
                    continue;
                };
                let context =
                    format!("built-in type `{package_name}.{interface_name}.{type_name}`");
                let directives =
                    parse_directives(type_def.docs.contents.as_deref(), &origin_path, &context)?;

                if let Some(existing) =
                    type_use_paths.insert(type_name.to_string(), use_path.clone())
                {
                    if existing != use_path {
                        return Err(Error::InvalidWit {
                            path: origin_path.join("model.wit"),
                            reason: format!(
                                "built-in type `{type_name}` is declared under multiple use paths"
                            ),
                        });
                    }
                }

                let Some(proto_name) =
                    directive_value(&directives, "proto", &origin_path, &context, "value")?
                else {
                    continue;
                };

                if let Some(existing) = proto_types.insert(
                    proto_name.clone(),
                    BuiltinTypeMetadata {
                        wit_name: type_name.to_string(),
                        use_path: use_path.clone(),
                    },
                ) {
                    return Err(Error::InvalidWit {
                        path: origin_path.join("model.wit"),
                        reason: format!(
                            "duplicate built-in `@nexus.proto` mapping for `{proto_name}` (`{}` and `{}`)",
                            existing.wit_name, type_name
                        ),
                    });
                }
            }
        }
    }

    Ok(BuiltinWitMetadata {
        proto_types,
        type_use_paths,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServiceSpec {
    pub name: String,
    pub endpoint: Option<String>,
    pub operations: Vec<OperationSpec>,
    pub resources: Vec<ResourceSpec>,
}

impl ServiceSpec {
    pub fn operation(&self, name: &str) -> Option<&OperationSpec> {
        self.operations
            .iter()
            .find(|operation| operation.name == name)
    }

    pub fn resource(&self, name: &str) -> Option<&ResourceSpec> {
        self.resources.iter().find(|resource| resource.name == name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SupportSpec {
    pub fragments: Vec<SupportFragmentSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupportFragmentSpec {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationSpec {
    pub name: String,
    pub input_proto: String,
    pub output_proto: String,
    pub output_resource: Option<String>,
    pub output_transform: Option<OperationOutputTransformSpec>,
}

impl OperationSpec {
    pub fn input_proto(&self) -> &str {
        &self.input_proto
    }

    pub fn output_proto(&self) -> &str {
        &self.output_proto
    }

    pub fn output_resource(&self) -> Option<&str> {
        self.output_resource.as_deref()
    }

    pub fn output_transform(&self) -> Option<&OperationOutputTransformSpec> {
        self.output_transform.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceSpec {
    pub name: String,
    pub fields: Vec<ResourceFieldSpec>,
    pub methods: Vec<ResourceMethodSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceFieldSpec {
    pub name: String,
    pub annotation: String,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceMethodSpec {
    pub name: String,
    pub params: Vec<ResourceFieldSpec>,
    pub result: Option<ResourceResultSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceResultSpec {
    pub annotation: String,
    pub proto: Option<String>,
    pub resource: Option<String>,
}

fn collect_support_spec(
    language: Language,
    resolve: &Resolve,
    current_package_id: PackageId,
    package_origins: &PackageOrigins,
) -> Result<SupportSpec> {
    let mut fragments = Vec::new();
    let mut seen_paths = BTreeSet::new();

    for (package_id, origin_path) in package_origins {
        if *package_id == current_package_id {
            continue;
        }
        collect_package_support_fragments(
            language,
            resolve,
            *package_id,
            origin_path,
            &mut seen_paths,
            &mut fragments,
        )?;
    }

    if let Some(origin_path) = package_origins.get(&current_package_id) {
        collect_package_support_fragments(
            language,
            resolve,
            current_package_id,
            origin_path,
            &mut seen_paths,
            &mut fragments,
        )?;
    }

    Ok(SupportSpec { fragments })
}

fn collect_package_support_fragments(
    language: Language,
    resolve: &Resolve,
    package_id: PackageId,
    origin_path: &Path,
    seen_paths: &mut BTreeSet<String>,
    fragments: &mut Vec<SupportFragmentSpec>,
) -> Result<()> {
    let package = &resolve.packages[package_id];
    let package_name = if let Some(version) = &package.name.version {
        format!(
            "{}:{}@{}",
            package.name.namespace, package.name.name, version
        )
    } else {
        format!("{}:{}", package.name.namespace, package.name.name)
    };

    collect_support_fragment_from_docs(
        language,
        package.docs.contents.as_deref(),
        origin_path,
        &format!("package `{package_name}`"),
        seen_paths,
        fragments,
    )?;

    for (world_name, world_id) in &package.worlds {
        let world = &resolve.worlds[*world_id];
        collect_support_fragment_from_docs(
            language,
            world.docs.contents.as_deref(),
            origin_path,
            &format!("package `{package_name}` world `{world_name}`"),
            seen_paths,
            fragments,
        )?;
    }

    Ok(())
}

fn collect_support_fragment_from_docs(
    language: Language,
    docs: Option<&str>,
    origin_path: &Path,
    context: &str,
    seen_paths: &mut BTreeSet<String>,
    fragments: &mut Vec<SupportFragmentSpec>,
) -> Result<()> {
    let directives = parse_directives(docs, origin_path, context)?;
    let Some(relative_path) =
        directive_value_for_language(&directives, "support", origin_path, context, language)?
    else {
        return Ok(());
    };

    let resolved_path = resolve_support_path(origin_path, &relative_path);
    let normalized_path = resolved_path.to_string_lossy().replace('\\', "/");
    if !seen_paths.insert(normalized_path.clone()) {
        return Ok(());
    }

    let contents = load_support_fragment_contents(&resolved_path)?;
    fragments.push(SupportFragmentSpec {
        path: normalized_path,
        contents,
    });
    Ok(())
}

fn load_support_fragment_contents(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::ReadFile {
        path: path.to_path_buf(),
        source,
    })
}

fn resolve_support_path(base_dir: &Path, support_path: &str) -> PathBuf {
    let support_path = PathBuf::from(support_path);
    if support_path.is_absolute() {
        support_path
    } else {
        base_dir.join(support_path)
    }
}

struct PreparedWitWorkspace {
    temp_dir: TempDir,
    package_root: PathBuf,
}

fn prepare_wit_workspace(input: &str, path: &Path) -> Result<PreparedWitWorkspace> {
    let temp_dir = tempfile::tempdir().map_err(|source| Error::WriteFile {
        path: PathBuf::from("<tempdir>"),
        source,
    })?;
    let package_root = temp_dir.path().join("main");
    fs::create_dir_all(&package_root).map_err(|source| Error::WriteFile {
        path: package_root.clone(),
        source,
    })?;

    if let Some(source_dir) = input_package_source_dir(path) {
        copy_package_source_dir(&source_dir, &package_root, path)?;
    } else if let Some(source_dir) = input_support_source_dir(path) {
        copy_standalone_input_support_dir(&source_dir, &package_root, path)?;
    }

    let target_name = input_target_name(path);
    let target_path = package_root.join(&target_name);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::WriteFile {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(&target_path, input).map_err(|source| Error::WriteFile {
        path: target_path,
        source,
    })?;

    copy_provided_builtins(&package_root)?;

    Ok(PreparedWitWorkspace {
        temp_dir,
        package_root,
    })
}

fn prepare_builtin_metadata_workspace() -> Result<PreparedWitWorkspace> {
    let temp_dir = tempfile::tempdir().map_err(|source| Error::WriteFile {
        path: PathBuf::from("<tempdir>"),
        source,
    })?;
    let package_root = temp_dir.path().join("main");
    fs::create_dir_all(&package_root).map_err(|source| Error::WriteFile {
        path: package_root.clone(),
        source,
    })?;
    let stub_path = package_root.join("main.wit");
    fs::write(
        &stub_path,
        "package temporary:root@0.0.0;\n\nworld system {\n}\n",
    )
    .map_err(|source| Error::WriteFile {
        path: stub_path,
        source,
    })?;
    copy_provided_builtins(&package_root)?;
    Ok(PreparedWitWorkspace {
        temp_dir,
        package_root,
    })
}

fn input_package_source_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return Some(path.to_path_buf());
    }

    if path.file_name()? != "main.wit" {
        return None;
    }

    let parent = path.parent()?;
    if parent.as_os_str().is_empty() || !parent.exists() {
        return None;
    }
    Some(parent.to_path_buf())
}

fn input_support_source_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return None;
    }

    let parent = path.parent()?;
    if parent.as_os_str().is_empty() || !parent.exists() {
        return None;
    }
    Some(parent.to_path_buf())
}

fn input_target_name(path: &Path) -> OsString {
    path.file_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| OsString::from("input.wit"))
}

fn copy_package_source_dir(
    source_dir: &Path,
    destination_dir: &Path,
    input_path: &Path,
) -> Result<()> {
    for entry in fs::read_dir(source_dir).map_err(|source| Error::ReadFile {
        path: source_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::ReadFile {
            path: source_dir.to_path_buf(),
            source,
        })?;
        let source_path = entry.path();
        let destination_path = destination_dir.join(entry.file_name());

        if source_path == input_path {
            continue;
        }

        let file_type = entry.file_type().map_err(|source| Error::ReadFile {
            path: source_path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            if entry.file_name() == "deps" {
                continue;
            }
            copy_package_source_dir(&source_path, &destination_path, input_path)?;
            continue;
        }

        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::WriteFile {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::copy(&source_path, &destination_path).map_err(|source| Error::WriteFile {
            path: destination_path,
            source,
        })?;
    }

    Ok(())
}

fn copy_standalone_input_support_dir(
    source_dir: &Path,
    destination_dir: &Path,
    input_path: &Path,
) -> Result<()> {
    for entry in fs::read_dir(source_dir).map_err(|source| Error::ReadFile {
        path: source_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::ReadFile {
            path: source_dir.to_path_buf(),
            source,
        })?;
        let source_path = entry.path();
        let destination_path = destination_dir.join(entry.file_name());

        if source_path == input_path {
            continue;
        }

        let file_type = entry.file_type().map_err(|source| Error::ReadFile {
            path: source_path.clone(),
            source,
        })?;
        if file_type.is_dir() {
            if entry.file_name() == "deps" {
                continue;
            }
            copy_standalone_input_support_dir(&source_path, &destination_path, input_path)?;
            continue;
        }

        if source_path
            .extension()
            .is_some_and(|extension| extension == "wit")
        {
            continue;
        }

        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::WriteFile {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::copy(&source_path, &destination_path).map_err(|source| Error::WriteFile {
            path: destination_path,
            source,
        })?;
    }

    Ok(())
}

fn copy_provided_builtins(package_root: &Path) -> Result<()> {
    let builtins_root = repo_builtins_root();
    if !builtins_root.exists() {
        return Ok(());
    }
    copy_directory_tree(&builtins_root, &package_root.join("deps"))
}

fn repo_builtins_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("builtins")
}

fn copy_directory_tree(source_dir: &Path, destination_dir: &Path) -> Result<()> {
    for entry in fs::read_dir(source_dir).map_err(|source| Error::ReadFile {
        path: source_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| Error::ReadFile {
            path: source_dir.to_path_buf(),
            source,
        })?;
        let source_path = entry.path();
        let destination_path = destination_dir.join(entry.file_name());
        let file_type = entry.file_type().map_err(|source| Error::ReadFile {
            path: source_path.clone(),
            source,
        })?;

        if file_type.is_dir() {
            fs::create_dir_all(&destination_path).map_err(|source| Error::WriteFile {
                path: destination_path.clone(),
                source,
            })?;
            copy_directory_tree(&source_path, &destination_path)?;
            continue;
        }

        if let Some(parent) = destination_path.parent() {
            fs::create_dir_all(parent).map_err(|source| Error::WriteFile {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::copy(&source_path, &destination_path).map_err(|source| Error::WriteFile {
            path: destination_path,
            source,
        })?;
    }
    Ok(())
}

fn collect_package_origins(
    resolve: &Resolve,
    source_map: &PackageSourceMap,
) -> Result<PackageOrigins> {
    let mut package_origins = BTreeMap::new();

    for (package_id, _) in resolve.packages.iter() {
        let Some(paths) = source_map.package_paths(package_id) else {
            continue;
        };
        let mut package_paths = paths.collect::<Vec<_>>();
        if package_paths.is_empty() {
            continue;
        }
        package_paths.sort();
        let origin = package_paths[0]
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        package_origins.insert(package_id, origin);
    }

    if package_origins.is_empty() {
        return Err(Error::InvalidWit {
            path: PathBuf::from("<workspace>"),
            reason: "resolved WIT package graph had no source origins".to_string(),
        });
    }

    Ok(package_origins)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationOutputTransformSpec {
    pub type_name: String,
    pub transform: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeOverrideSpec {
    pub required_fields: BTreeSet<String>,
    pub omitted_fields: BTreeSet<String>,
    pub replacement: Option<TypeReplacementSpec>,
    pub generated_model: GeneratedModelSpec,
}

impl TypeOverrideSpec {
    pub fn is_field_required(&self, field_name: &str) -> bool {
        self.required_fields.contains(field_name)
    }

    pub fn is_field_omitted(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
    }

    pub fn is_field_hidden(&self, field_name: &str) -> bool {
        self.omitted_fields.contains(field_name)
            || self.field_source(field_name).is_some()
            || (!self.generated_model.declared_fields.is_empty()
                && !self.generated_model.declared_fields.contains(field_name))
    }

    pub fn replacement(&self) -> Option<&TypeReplacementSpec> {
        self.replacement.as_ref()
    }

    pub fn generated_model(&self) -> Option<&GeneratedModelSpec> {
        if self.generated_model.is_empty() {
            None
        } else {
            Some(&self.generated_model)
        }
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.generated_model()?.field_source(field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeReplacementSpec {
    pub type_name: String,
    pub from_proto: Option<String>,
    pub to_proto: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GeneratedModelSpec {
    pub declared_fields: BTreeSet<String>,
    pub field_names: BTreeMap<String, String>,
    pub field_annotations: BTreeMap<String, String>,
    pub field_sources: BTreeMap<String, String>,
    pub functions: BTreeMap<String, FunctionFieldSpec>,
    pub with_arguments: BTreeMap<String, WithArgumentsFieldSpec>,
}

impl GeneratedModelSpec {
    pub fn is_empty(&self) -> bool {
        self.declared_fields.is_empty()
            && self.field_names.is_empty()
            && self.field_annotations.is_empty()
            && self.field_sources.is_empty()
            && self.functions.is_empty()
            && self.with_arguments.is_empty()
    }

    pub fn field_name_override(&self, field_name: &str) -> Option<&str> {
        self.field_names.get(field_name).map(String::as_str)
    }

    pub fn field_annotation(&self, field_name: &str) -> Option<&str> {
        self.field_annotations.get(field_name).map(String::as_str)
    }

    pub fn field_source(&self, field_name: &str) -> Option<&str> {
        self.field_sources.get(field_name).map(String::as_str)
    }

    pub fn function(&self, field_name: &str) -> Option<&FunctionFieldSpec> {
        self.functions.get(field_name)
    }

    pub fn function_for_args_field(&self, field_name: &str) -> Option<&FunctionFieldSpec> {
        self.functions
            .values()
            .find(|function| function.args_field == field_name)
    }

    pub fn with_arguments(&self, field_name: &str) -> Option<&WithArgumentsFieldSpec> {
        self.with_arguments.get(field_name)
    }

    pub fn with_arguments_for_args_field(
        &self,
        field_name: &str,
    ) -> Option<&WithArgumentsFieldSpec> {
        self.with_arguments
            .values()
            .find(|with_arguments| with_arguments.args_field == field_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionFieldSpec {
    pub primary: bool,
    pub result_type: String,
    pub args_field: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithArgumentsFieldSpec {
    pub args_field: String,
    pub value_type: String,
    pub args_type: String,
    pub name_expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlattenedFunctionTypeSpec {
    args_name: String,
    function: Option<FunctionFieldSpec>,
    with_arguments: Option<WithArgumentsFieldSpec>,
}

fn collect_interface_types(
    language: Language,
    resolve: &Resolve,
    interface: &Interface,
    path: &Path,
    types: &mut BTreeMap<String, TypeOverrideSpec>,
) -> Result<()> {
    let interface_name = interface
        .name
        .as_deref()
        .unwrap_or("unnamed-interface")
        .to_string();
    for type_id in interface.types.values() {
        let type_def = &resolve.types[*type_id];
        let Some((proto_name, type_override)) =
            build_type_override(language, resolve, type_def, path, &interface_name)?
        else {
            continue;
        };
        if types.insert(proto_name.clone(), type_override).is_some() {
            return Err(Error::InvalidWit {
                path: path.to_path_buf(),
                reason: format!("duplicate `@nexus.proto` mapping for `{proto_name}`"),
            });
        }
    }

    Ok(())
}

fn build_type_override(
    language: Language,
    resolve: &Resolve,
    type_def: &TypeDef,
    path: &Path,
    interface_name: &str,
) -> Result<Option<(String, TypeOverrideSpec)>> {
    let type_name = type_def.name.as_deref().unwrap_or("unnamed-type");
    let context = format!("type `{interface_name}.{type_name}`");
    let directives = parse_directives(type_def.docs.contents.as_deref(), path, &context)?;
    let Some(proto_name) = directive_value(&directives, "proto", path, &context, "value")? else {
        return Ok(None);
    };

    let replacement = build_type_replacement(language, &directives, path, &context, &proto_name)?;

    let (required_fields, generated_model) = match &type_def.kind {
        TypeDefKind::Record(record) => {
            build_generated_model_from_record(language, resolve, record, path, &context)?
        }
        _ => (BTreeSet::new(), GeneratedModelSpec::default()),
    };

    let type_override = TypeOverrideSpec {
        required_fields,
        omitted_fields: BTreeSet::new(),
        replacement,
        generated_model,
    };

    Ok(Some((proto_name, type_override)))
}

fn build_generated_model_from_record(
    language: Language,
    resolve: &Resolve,
    record: &wit_parser::Record,
    path: &Path,
    context: &str,
) -> Result<(BTreeSet<String>, GeneratedModelSpec)> {
    let mut required_fields = BTreeSet::new();
    let mut declared_fields = BTreeSet::new();
    let mut field_names = BTreeMap::new();
    let mut field_annotations = BTreeMap::new();
    let mut field_sources = BTreeMap::new();
    let mut functions = BTreeMap::new();
    let mut with_arguments = BTreeMap::new();

    for field in &record.fields {
        let field_context = format!("{context} field `{}`", field.name);
        let directives = parse_directives(field.docs.contents.as_deref(), path, &field_context)?;
        let proto_field_name =
            directive_value(&directives, "proto-field", path, &field_context, "value")?
                .unwrap_or_else(|| field.name.to_snake_case());
        let flattened_function_type = if directive(&directives, "function", path, &field_context)?
            .is_none()
            && directive(&directives, "with-arguments", path, &field_context)?.is_none()
        {
            find_flattened_function_type_spec(language, resolve, &field.ty, path)?
        } else {
            None
        };

        if !declared_fields.insert(proto_field_name.clone()) {
            return Err(Error::InvalidWit {
                path: path.to_path_buf(),
                reason: format!(
                    "{field_context} maps to duplicate proto field `{proto_field_name}`"
                ),
            });
        }

        field_names.insert(proto_field_name.clone(), field.name.clone());

        if !is_optional_type(resolve, &field.ty) {
            required_fields.insert(proto_field_name.clone());
        }

        if let Some(source) = find_language_directive_value(
            language,
            &[field.docs.contents.as_deref()],
            path,
            &field_context,
            "source",
        )? {
            field_sources.insert(proto_field_name.clone(), source);
        }

        let field_annotation = if let Some(annotation) = find_language_directive_value(
            language,
            &[field.docs.contents.as_deref()],
            path,
            &field_context,
            "type",
        )? {
            Some(annotation)
        } else {
            find_language_type_annotation_for_field_type(language, resolve, &field.ty, path)?
        };
        if let Some(annotation) = field_annotation {
            field_annotations.insert(proto_field_name.clone(), annotation);
        }

        if let Some(function) = build_function_field(language, &directives, path, &field_context)? {
            functions.insert(proto_field_name.clone(), function);
        }

        if let Some(with_arguments_field) =
            build_with_arguments_field(language, &directives, path, &field_context)?
        {
            with_arguments.insert(proto_field_name.clone(), with_arguments_field);
        }

        if field_sources.contains_key(&proto_field_name)
            && functions.contains_key(&proto_field_name)
        {
            return Err(Error::ConflictingTypeOverrideFieldProperties {
                message: context.to_string(),
                field: proto_field_name,
                property: "source",
                conflicting_property: "function",
            });
        }

        if let Some(flattened_function_type) = flattened_function_type {
            let args_proto_field_name = flattened_function_type.args_name.to_snake_case();
            if !declared_fields.insert(args_proto_field_name.clone()) {
                return Err(Error::InvalidWit {
                    path: path.to_path_buf(),
                    reason: format!(
                        "{field_context} maps to duplicate proto field `{args_proto_field_name}`"
                    ),
                });
            }
            field_names.insert(
                args_proto_field_name.clone(),
                flattened_function_type.args_name.clone(),
            );
            if let Some(function) = flattened_function_type.function {
                functions.insert(proto_field_name.clone(), function);
            }
            if let Some(with_arguments_field) = flattened_function_type.with_arguments {
                with_arguments.insert(proto_field_name.clone(), with_arguments_field);
            }
        }
    }

    Ok((
        required_fields,
        GeneratedModelSpec {
            declared_fields,
            field_names,
            field_annotations,
            field_sources,
            functions,
            with_arguments,
        },
    ))
}

fn build_type_replacement(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
    type_name: &str,
) -> Result<Option<TypeReplacementSpec>> {
    let Some(directive) = directive(directives, "type", path, context)? else {
        return Ok(None);
    };

    let selected_from_proto = directive
        .value(&format!("{}-from", language_key(language)))
        .map(ToOwned::to_owned);
    let selected_to_proto = directive
        .value(&format!("{}-to", language_key(language)))
        .map(ToOwned::to_owned);
    let selected_type_name =
        directive_language_value(directive, language).or_else(|| directive.value("value"));

    let Some(selected_type_name) = selected_type_name else {
        if selected_from_proto.is_some() || selected_to_proto.is_some() {
            return Err(Error::IncompleteTypeOverride {
                type_name: type_name.to_string(),
            });
        }
        return Ok(None);
    };

    Ok(Some(TypeReplacementSpec {
        type_name: selected_type_name.to_string(),
        from_proto: selected_from_proto,
        to_proto: selected_to_proto,
    }))
}

fn find_language_type_annotation_for_field_type(
    language: Language,
    resolve: &Resolve,
    ty: &Type,
    path: &Path,
) -> Result<Option<String>> {
    let mut current = ty;
    loop {
        match current {
            Type::Id(id) => {
                let type_def = &resolve.types[*id];
                let type_name = type_def.name.as_deref().unwrap_or("unnamed-type");
                let context = format!("type `{type_name}`");
                if let Some(annotation) = find_language_directive_value(
                    language,
                    &[type_def.docs.contents.as_deref()],
                    path,
                    &context,
                    "type",
                )? {
                    return Ok(Some(annotation));
                }
                match &type_def.kind {
                    TypeDefKind::Type(next) => current = next,
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        }
    }
}

fn resolve_wit_type_annotation(
    language: Language,
    resolve: &Resolve,
    ty: &Type,
    path: &Path,
    context: &str,
) -> Result<String> {
    match ty {
        Type::Bool => Ok(match language {
            Language::Python => "bool".to_string(),
            Language::TypeScript => "boolean".to_string(),
            _ => "bool".to_string(),
        }),
        Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::S8
        | Type::S16
        | Type::S32
        | Type::S64 => Ok(match language {
            Language::Python => "int".to_string(),
            Language::TypeScript => "number".to_string(),
            _ => "int".to_string(),
        }),
        Type::F32 | Type::F64 => Ok(match language {
            Language::Python => "float".to_string(),
            Language::TypeScript => "number".to_string(),
            _ => "float".to_string(),
        }),
        Type::Char | Type::String => Ok(match language {
            Language::Python => "str".to_string(),
            Language::TypeScript => "string".to_string(),
            _ => "string".to_string(),
        }),
        Type::Id(id) => {
            let type_def = &resolve.types[*id];
            let type_name = type_def.name.as_deref().unwrap_or("unnamed-type");
            let type_context = format!("{context} type `{type_name}`");
            if let Some(annotation) = find_language_directive_value(
                language,
                &[type_def.docs.contents.as_deref()],
                path,
                &type_context,
                "type",
            )? {
                return Ok(annotation);
            }
            if let Some(resource_name) = find_owned_resource_name_for_type_def(resolve, type_def) {
                return Ok(resource_name.to_upper_camel_case());
            }
            match &type_def.kind {
                TypeDefKind::Option(inner) => {
                    let inner_annotation =
                        resolve_wit_type_annotation(language, resolve, inner, path, &type_context)?;
                    Ok(match language {
                        Language::Python => format!("{inner_annotation} | None"),
                        Language::TypeScript => format!("{inner_annotation} | undefined"),
                        _ => inner_annotation,
                    })
                }
                TypeDefKind::List(inner) => {
                    let inner_annotation =
                        resolve_wit_type_annotation(language, resolve, inner, path, &type_context)?;
                    Ok(match language {
                        Language::Python => format!("list[{inner_annotation}]"),
                        Language::TypeScript => format!("{inner_annotation}[]"),
                        _ => inner_annotation,
                    })
                }
                TypeDefKind::Tuple(tuple) => {
                    let item_annotations = tuple
                        .types
                        .iter()
                        .map(|item| {
                            resolve_wit_type_annotation(
                                language,
                                resolve,
                                item,
                                path,
                                &type_context,
                            )
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Ok(match language {
                        Language::Python => format!("tuple[{}]", item_annotations.join(", ")),
                        Language::TypeScript => format!("[{}]", item_annotations.join(", ")),
                        _ => item_annotations.join(", "),
                    })
                }
                TypeDefKind::Type(next) => {
                    resolve_wit_type_annotation(language, resolve, next, path, &type_context)
                }
                TypeDefKind::Handle(Handle::Own(resource_id))
                | TypeDefKind::Handle(Handle::Borrow(resource_id)) => {
                    let resource_def = &resolve.types[*resource_id];
                    let resource_name = resource_def.name.as_deref().unwrap_or("unnamed-resource");
                    Ok(resource_name.to_upper_camel_case())
                }
                TypeDefKind::Resource => Ok(type_name.to_upper_camel_case()),
                TypeDefKind::Record(_)
                | TypeDefKind::Flags(_)
                | TypeDefKind::Variant(_)
                | TypeDefKind::Enum(_)
                | TypeDefKind::Map(_, _)
                | TypeDefKind::FixedSizeList(_, _)
                | TypeDefKind::Result(_)
                | TypeDefKind::Future(_)
                | TypeDefKind::Stream(_) => {
                    let proto_name =
                        find_proto_name_for_type_def(type_def, path, &type_context)?.ok_or_else(
                            || Error::InvalidWit {
                                path: path.to_path_buf(),
                                reason: format!(
                                    "{type_context} must provide either `@nexus.type` or `@nexus.proto` for generated resource bindings"
                                ),
                            },
                        )?;
                    Ok(proto_name
                        .rsplit('.')
                        .next()
                        .expect("proto names should have a final segment")
                        .to_string())
                }
                _ => Err(Error::InvalidWit {
                    path: path.to_path_buf(),
                    reason: format!(
                        "{type_context} uses unsupported WIT type `{}` for generated resource bindings",
                        type_def.kind.as_str()
                    ),
                }),
            }
        }
        _ => Err(Error::InvalidWit {
            path: path.to_path_buf(),
            reason: format!("{context} uses unsupported WIT type for generated resource bindings"),
        }),
    }
}

fn find_proto_name_for_type(
    resolve: &Resolve,
    ty: &Type,
    path: &Path,
    context: &str,
) -> Result<Option<String>> {
    let mut current = ty;
    loop {
        match current {
            Type::Id(id) => {
                let type_def = &resolve.types[*id];
                let type_name = type_def.name.as_deref().unwrap_or("unnamed-type");
                let type_context = format!("{context} type `{type_name}`");
                let directives =
                    parse_directives(type_def.docs.contents.as_deref(), path, &type_context)?;
                if let Some(proto_name) =
                    directive_value(&directives, "proto", path, &type_context, "value")?
                {
                    return Ok(Some(proto_name));
                }
                match &type_def.kind {
                    TypeDefKind::Type(next) => current = next,
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        }
    }
}

fn find_owned_resource_name_for_type(resolve: &Resolve, ty: &Type) -> Option<String> {
    match ty {
        Type::Id(id) => find_owned_resource_name_for_type_def(resolve, &resolve.types[*id]),
        _ => None,
    }
}

fn find_owned_resource_name_for_type_def(resolve: &Resolve, type_def: &TypeDef) -> Option<String> {
    match &type_def.kind {
        TypeDefKind::Handle(Handle::Own(resource_id)) => resolve.types[*resource_id]
            .name
            .as_deref()
            .map(str::to_string),
        TypeDefKind::Type(next) => find_owned_resource_name_for_type(resolve, next),
        _ => None,
    }
}

pub(crate) fn find_proto_name_for_type_def(
    type_def: &TypeDef,
    path: &Path,
    context: &str,
) -> Result<Option<String>> {
    let directives = parse_directives(type_def.docs.contents.as_deref(), path, context)?;
    directive_value(&directives, "proto", path, context, "value")
}

fn build_function_field(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
) -> Result<Option<FunctionFieldSpec>> {
    let Some(directive) = directive(directives, "function", path, context)? else {
        return Ok(None);
    };

    let Some(result_type) = directive
        .value(&format!("{}-result", language_key(language)))
        .or_else(|| directive.value("result"))
    else {
        return Ok(None);
    };

    let Some(args_field) = directive.value("args-field") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.function".to_string(),
            reason: "missing required `args-field`".to_string(),
        });
    };

    let primary = directive
        .value("primary")
        .map(parse_bool)
        .transpose()
        .map_err(|reason| Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.function".to_string(),
            reason,
        })?
        .unwrap_or(false);

    Ok(Some(FunctionFieldSpec {
        primary,
        result_type: result_type.to_string(),
        args_field: args_field.to_snake_case(),
    }))
}

fn build_function_field_with_args_key(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
    args_key: &str,
) -> Result<Option<FunctionFieldSpec>> {
    let Some(directive) = directive(directives, "function", path, context)? else {
        return Ok(None);
    };

    let Some(result_type) = directive
        .value(&format!("{}-result", language_key(language)))
        .or_else(|| directive.value("result"))
    else {
        return Ok(None);
    };

    let Some(args_field) = directive.value(args_key) else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.function".to_string(),
            reason: format!("missing required `{args_key}`"),
        });
    };

    let primary = directive
        .value("primary")
        .map(parse_bool)
        .transpose()
        .map_err(|reason| Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.function".to_string(),
            reason,
        })?
        .unwrap_or(false);

    Ok(Some(FunctionFieldSpec {
        primary,
        result_type: result_type.to_string(),
        args_field: args_field.to_snake_case(),
    }))
}

fn build_with_arguments_field(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
) -> Result<Option<WithArgumentsFieldSpec>> {
    if language != Language::TypeScript {
        return Ok(None);
    }

    let Some(directive) = directive(directives, "with-arguments", path, context)? else {
        return Ok(None);
    };

    let Some(args_field) = directive.value("args-field") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `args-field`".to_string(),
        });
    };
    let Some(value_type) = directive.value("value-type") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `value-type`".to_string(),
        });
    };
    let Some(args_type) = directive.value("args-type") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `args-type`".to_string(),
        });
    };
    let Some(name_expr) = directive.value("name-expr") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `name-expr`".to_string(),
        });
    };

    Ok(Some(WithArgumentsFieldSpec {
        args_field: args_field.to_snake_case(),
        value_type: value_type.to_string(),
        args_type: args_type.to_string(),
        name_expr: name_expr.to_string(),
    }))
}

fn build_with_arguments_field_with_args_key(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
    args_key: &str,
) -> Result<Option<WithArgumentsFieldSpec>> {
    if language != Language::TypeScript {
        return Ok(None);
    }

    let Some(directive) = directive(directives, "with-arguments", path, context)? else {
        return Ok(None);
    };

    let Some(args_field) = directive.value(args_key) else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: format!("missing required `{args_key}`"),
        });
    };
    let Some(value_type) = directive.value("value-type") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `value-type`".to_string(),
        });
    };
    let Some(args_type) = directive.value("args-type") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `args-type`".to_string(),
        });
    };
    let Some(name_expr) = directive.value("name-expr") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: "@nexus.with-arguments".to_string(),
            reason: "missing required `name-expr`".to_string(),
        });
    };

    Ok(Some(WithArgumentsFieldSpec {
        args_field: args_field.to_snake_case(),
        value_type: value_type.to_string(),
        args_type: args_type.to_string(),
        name_expr: name_expr.to_string(),
    }))
}

fn find_flattened_function_type_spec(
    language: Language,
    resolve: &Resolve,
    ty: &Type,
    path: &Path,
) -> Result<Option<FlattenedFunctionTypeSpec>> {
    let mut current = ty;
    loop {
        match current {
            Type::Id(id) => {
                let type_def = &resolve.types[*id];
                let type_name = type_def.name.as_deref().unwrap_or("unnamed-type");
                let context = format!("type `{type_name}`");
                let directives =
                    parse_directives(type_def.docs.contents.as_deref(), path, &context)?;
                let function = build_function_field_with_args_key(
                    language,
                    &directives,
                    path,
                    &context,
                    "args-name",
                )?;
                let with_arguments = build_with_arguments_field_with_args_key(
                    language,
                    &directives,
                    path,
                    &context,
                    "args-name",
                )?;
                if function.is_some() || with_arguments.is_some() {
                    let args_name = directive_value(
                        &directives,
                        if function.is_some() {
                            "function"
                        } else {
                            "with-arguments"
                        },
                        path,
                        &context,
                        "args-name",
                    )?
                    .expect("args-name validated when building flattened function type");
                    return Ok(Some(FlattenedFunctionTypeSpec {
                        args_name,
                        function,
                        with_arguments,
                    }));
                }
                match &type_def.kind {
                    TypeDefKind::Type(next) => current = next,
                    _ => return Ok(None),
                }
            }
            _ => return Ok(None),
        }
    }
}

fn build_service(
    language: Language,
    resolve: &Resolve,
    key: &WorldKey,
    interface: &Interface,
    path: &Path,
) -> Result<ServiceSpec> {
    let interface_name = interface_export_name(key, interface);
    let context = format!("interface `{interface_name}`");
    let directives = parse_directives(interface.docs.contents.as_deref(), path, &context)?;
    let endpoint = directive_value(&directives, "endpoint", path, &context, "value")?;
    let service_name = interface_name.to_upper_camel_case();

    let operations = interface
        .functions
        .iter()
        .filter(|(_, function)| {
            matches!(
                function.kind,
                FunctionKind::Freestanding | FunctionKind::AsyncFreestanding
            )
        })
        .map(|(_, function)| {
            build_operation(language, resolve, function, path, &context, &service_name)
        })
        .collect::<Result<Vec<_>>>()?;

    let mut resources = Vec::new();
    for type_id in interface.types.values() {
        let type_def = &resolve.types[*type_id];
        if !matches!(type_def.kind, TypeDefKind::Resource) {
            continue;
        }
        resources.push(build_resource(
            language, resolve, interface, *type_id, type_def, path, &context,
        )?);
    }

    Ok(ServiceSpec {
        name: service_name,
        endpoint,
        operations,
        resources,
    })
}

fn build_resource(
    language: Language,
    resolve: &Resolve,
    interface: &Interface,
    resource_id: wit_parser::TypeId,
    type_def: &TypeDef,
    path: &Path,
    service_context: &str,
) -> Result<ResourceSpec> {
    let resource_name = type_def
        .name
        .as_deref()
        .ok_or_else(|| Error::InvalidWit {
            path: path.to_path_buf(),
            reason: format!("{service_context} declares an unnamed resource"),
        })?
        .to_string();
    let context = format!(
        "{service_context} resource `{}`",
        resource_name.to_upper_camel_case()
    );

    let constructor = interface.functions.values().find(
        |function| matches!(function.kind, FunctionKind::Constructor(id) if id == resource_id),
    );
    let fields = match constructor {
        Some(constructor) => constructor
            .params
            .iter()
            .map(|(name, ty)| {
                build_resource_field(language, resolve, name, ty, path, &context, "constructor")
            })
            .collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };

    let methods = interface
        .functions
        .values()
        .filter(|function| {
            matches!(
                function.kind,
                FunctionKind::Method(id) | FunctionKind::AsyncMethod(id) if id == resource_id
            )
        })
        .map(|function| build_resource_method(language, resolve, function, path, &context))
        .collect::<Result<Vec<_>>>()?;

    for function in interface.functions.values() {
        match function.kind {
            FunctionKind::Static(id) | FunctionKind::AsyncStatic(id) if id == resource_id => {
                return Err(Error::InvalidWit {
                    path: path.to_path_buf(),
                    reason: format!(
                        "{context} static methods are not supported yet (`{}`)",
                        function.name
                    ),
                });
            }
            _ => {}
        }
    }

    Ok(ResourceSpec {
        name: resource_name,
        fields,
        methods,
    })
}

fn build_resource_method(
    language: Language,
    resolve: &Resolve,
    function: &Function,
    path: &Path,
    resource_context: &str,
) -> Result<ResourceMethodSpec> {
    let method_name = function
        .name
        .rsplit('.')
        .next()
        .unwrap_or(function.name.as_str())
        .to_string();
    let context = format!(
        "{resource_context} method `{}`",
        method_name.to_upper_camel_case()
    );
    let params = function
        .params
        .iter()
        .skip_while(|(name, _)| name == "self")
        .map(|(name, ty)| {
            build_resource_field(language, resolve, name, ty, path, &context, "parameter")
        })
        .collect::<Result<Vec<_>>>()?;
    let result = function
        .result
        .as_ref()
        .map(|ty| build_resource_result(language, resolve, ty, path, &context))
        .transpose()?;

    Ok(ResourceMethodSpec {
        name: method_name,
        params,
        result,
    })
}

fn build_resource_result(
    language: Language,
    resolve: &Resolve,
    ty: &Type,
    path: &Path,
    context: &str,
) -> Result<ResourceResultSpec> {
    Ok(ResourceResultSpec {
        annotation: resolve_wit_type_annotation(language, resolve, ty, path, context)?,
        proto: find_proto_name_for_type(resolve, ty, path, context)?,
        resource: find_owned_resource_name_for_type(resolve, ty),
    })
}

fn build_resource_field(
    language: Language,
    resolve: &Resolve,
    name: &str,
    ty: &Type,
    path: &Path,
    context: &str,
    _role: &str,
) -> Result<ResourceFieldSpec> {
    let annotation = resolve_wit_type_annotation(language, resolve, ty, path, context)?;
    Ok(ResourceFieldSpec {
        name: name.to_string(),
        annotation,
        optional: is_optional_type(resolve, ty),
    })
}

fn build_operation(
    language: Language,
    resolve: &Resolve,
    function: &Function,
    path: &Path,
    service_context: &str,
    service_name: &str,
) -> Result<OperationSpec> {
    let operation_name = function.name.to_upper_camel_case();
    let context = format!("{service_context} operation `{operation_name}`");
    let directives = parse_directives(function.docs.contents.as_deref(), path, &context)?;

    let [(parameter_name, input_type)] = function.params.as_slice() else {
        return Err(Error::InvalidWit {
            path: path.to_path_buf(),
            reason: format!("{context} must declare exactly one input parameter"),
        });
    };
    let input_proto =
        find_proto_name_for_type(resolve, input_type, path, &context)?.ok_or_else(|| {
            Error::InvalidWit {
                path: path.to_path_buf(),
                reason: format!(
                    "{context} parameter `{parameter_name}` type must resolve to a type annotated with `@nexus.proto`"
                ),
            }
        })?;
    let output_type = function.result.as_ref().ok_or_else(|| Error::InvalidWit {
        path: path.to_path_buf(),
        reason: format!("{context} must declare a result type"),
    })?;
    let output_proto =
        find_proto_name_for_type(resolve, output_type, path, &context)?.ok_or_else(|| {
            Error::InvalidWit {
                path: path.to_path_buf(),
                reason: format!(
                    "{context} result type must resolve to a type annotated with `@nexus.proto`"
                ),
            }
        })?;
    let output_resource = find_owned_resource_name_for_type(resolve, output_type);

    let output_transform = build_operation_output_transform(
        language,
        &directives,
        path,
        &context,
        service_name,
        &operation_name,
    )?;

    Ok(OperationSpec {
        name: operation_name,
        input_proto,
        output_proto,
        output_resource,
        output_transform,
    })
}

fn build_operation_output_transform(
    language: Language,
    directives: &[Directive],
    path: &Path,
    context: &str,
    service_name: &str,
    operation_name: &str,
) -> Result<Option<OperationOutputTransformSpec>> {
    let Some(directive) = directive(directives, "output-transform", path, context)? else {
        return Ok(None);
    };

    let type_key = format!("{}-type", language_key(language));
    let type_name = directive.value(&type_key);
    let transform = directive_language_value(directive, language);

    match (type_name, transform) {
        (None, None) => Ok(None),
        (Some(type_name), Some(transform)) => Ok(Some(OperationOutputTransformSpec {
            type_name: type_name.to_string(),
            transform: transform.to_string(),
        })),
        _ => Err(Error::IncompleteOperationOutputTransform {
            service: service_name.to_string(),
            operation: operation_name.to_string(),
        }),
    }
}

pub(crate) fn select_world(
    resolve: &Resolve,
    package_id: PackageId,
    path: &Path,
) -> Result<wit_parser::WorldId> {
    let package = &resolve.packages[package_id];
    match package.worlds.len() {
        1 => Ok(*package
            .worlds
            .values()
            .next()
            .expect("world map length checked")),
        0 => Err(Error::InvalidWit {
            path: path.to_path_buf(),
            reason: "package must declare exactly one world".to_string(),
        }),
        _ => Err(Error::InvalidWit {
            path: path.to_path_buf(),
            reason: "package declares multiple worlds; choose one world per input".to_string(),
        }),
    }
}

fn interface_export_name(key: &WorldKey, interface: &Interface) -> String {
    match key {
        WorldKey::Name(name) => name.clone(),
        WorldKey::Interface(_) => interface
            .name
            .clone()
            .unwrap_or_else(|| "unnamed-interface".to_string()),
    }
}

fn is_optional_type(resolve: &Resolve, ty: &Type) -> bool {
    let mut current = ty;
    loop {
        match current {
            Type::Id(id) => match &resolve.types[*id].kind {
                TypeDefKind::Option(_) => return true,
                TypeDefKind::Type(next) => current = next,
                _ => return false,
            },
            _ => return false,
        }
    }
}

fn find_language_directive_value(
    language: Language,
    docs: &[Option<&str>],
    path: &Path,
    context: &str,
    directive_name: &str,
) -> Result<Option<String>> {
    for docs in docs {
        let directives = parse_directives(*docs, path, context)?;
        if let Some(value) =
            directive_value_for_language(&directives, directive_name, path, context, language)?
        {
            return Ok(Some(value));
        }
    }

    Ok(None)
}

fn directive_value_for_language(
    directives: &[Directive],
    name: &str,
    path: &Path,
    context: &str,
    language: Language,
) -> Result<Option<String>> {
    let Some(directive) = directive(directives, name, path, context)? else {
        return Ok(None);
    };
    Ok(directive_language_value(directive, language)
        .or_else(|| directive.value("value"))
        .map(ToOwned::to_owned))
}

fn directive_value(
    directives: &[Directive],
    name: &str,
    path: &Path,
    context: &str,
    key: &str,
) -> Result<Option<String>> {
    Ok(directive(directives, name, path, context)?
        .and_then(|directive| directive.value(key))
        .map(ToOwned::to_owned))
}

fn directive<'a>(
    directives: &'a [Directive],
    name: &str,
    path: &Path,
    context: &str,
) -> Result<Option<&'a Directive>> {
    let mut matches = directives.iter().filter(|directive| directive.name == name);
    let first = matches.next();
    if matches.next().is_some() {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: format!("@nexus.{name}"),
            reason: "duplicate directive".to_string(),
        });
    }
    Ok(first)
}

fn directive_language_value<'a>(directive: &'a Directive, language: Language) -> Option<&'a str> {
    directive.value(language_key(language))
}

fn language_key(language: Language) -> &'static str {
    match language {
        Language::Dotnet => "dotnet",
        Language::Go => "go",
        Language::Java => "java",
        Language::Python => "python",
        Language::Ruby => "ruby",
        Language::TypeScript => "typescript",
    }
}

fn parse_directives(docs: Option<&str>, path: &Path, context: &str) -> Result<Vec<Directive>> {
    let Some(docs) = docs else {
        return Ok(Vec::new());
    };

    let mut directives = Vec::new();
    let mut current = None::<String>;

    for line in docs.lines() {
        let trimmed_start = line.trim_start();
        if trimmed_start.starts_with("@nexus.") {
            if let Some(previous) = current.take() {
                directives.push(parse_directive_line(&previous, path, context)?);
            }
            current = Some(trimmed_start.to_string());
            continue;
        }

        let is_continuation = current.is_some()
            && !trimmed_start.is_empty()
            && trimmed_start.len() != line.len()
            && (trimmed_start.starts_with('"') || trimmed_start.contains('='));

        if is_continuation {
            let directive = current
                .as_mut()
                .expect("continuation checked to have an active directive");
            directive.push(' ');
            directive.push_str(trimmed_start);
            continue;
        }

        if let Some(previous) = current.take() {
            directives.push(parse_directive_line(&previous, path, context)?);
        }
    }

    if let Some(previous) = current.take() {
        directives.push(parse_directive_line(&previous, path, context)?);
    }

    Ok(directives)
}

#[derive(Debug, Clone)]
struct Directive {
    name: String,
    args: BTreeMap<String, String>,
}

impl Directive {
    fn value(&self, key: &str) -> Option<&str> {
        self.args.get(key).map(String::as_str)
    }
}

fn parse_directive_line(line: &str, path: &Path, context: &str) -> Result<Directive> {
    let Some(rest) = line.strip_prefix("@nexus.") else {
        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: line.to_string(),
            reason: "directive must start with `@nexus.`".to_string(),
        });
    };

    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let name = &rest[..name_end];
    let mut tail = rest[name_end..].trim_start();
    let mut args = BTreeMap::new();

    if tail.starts_with('"') {
        let (value, remaining) = parse_directive_value(tail, path, context, name)?;
        args.insert("value".to_string(), value);
        tail = remaining.trim_start();
    }

    while !tail.is_empty() {
        let key_end = tail
            .find(|character: char| character == '=' || character.is_whitespace())
            .unwrap_or(tail.len());
        let key = &tail[..key_end];
        let after_key = tail[key_end..].trim_start();
        let Some(after_equals) = after_key.strip_prefix('=') else {
            return Err(Error::InvalidWitDirective {
                path: path.to_path_buf(),
                context: context.to_string(),
                directive: format!("@nexus.{name}"),
                reason: format!("expected `=` after `{key}`"),
            });
        };
        let (value, remaining) =
            parse_directive_value(after_equals.trim_start(), path, context, name)?;
        args.insert(key.to_string(), value);
        tail = remaining.trim_start();
    }

    Ok(Directive {
        name: name.to_string(),
        args,
    })
}

fn parse_directive_value<'a>(
    input: &'a str,
    path: &Path,
    context: &str,
    name: &str,
) -> Result<(String, &'a str)> {
    if let Some(stripped) = input.strip_prefix('"') {
        let mut escaped = false;
        let mut value = String::new();
        for (index, character) in stripped.char_indices() {
            if escaped {
                value.push(character);
                escaped = false;
                continue;
            }
            match character {
                '\\' => escaped = true,
                '"' => return Ok((value, &stripped[index + 1..])),
                _ => value.push(character),
            }
        }

        return Err(Error::InvalidWitDirective {
            path: path.to_path_buf(),
            context: context.to_string(),
            directive: format!("@nexus.{name}"),
            reason: "unterminated quoted string".to_string(),
        });
    }

    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    Ok((input[..end].to_string(), &input[end..]))
}

fn parse_bool(value: &str) -> std::result::Result<bool, String> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(format!("expected `true` or `false`, found `{value}`")),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::descriptors::DescriptorIndex;
    use crate::error::Error;
    use crate::language::Language;

    use super::{ApiSpec, directive, load_builtin_wit_metadata, parse_directives};

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn descriptors() -> DescriptorIndex {
        DescriptorIndex::load(&root().join("examples/descriptors/temporal_api.bin")).unwrap()
    }

    fn parse(language: Language, wit: &str) -> ApiSpec {
        ApiSpec::parse_for_language(language, wit, PathBuf::from("inline.wit")).unwrap()
    }

    fn validate(language: Language, wit: &str) -> Result<(), Error> {
        let spec = parse(language, wit);
        let descriptors = descriptors();
        crate::validation::validate_type_overrides(&spec, &descriptors, language)
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nexus-api-gen-{label}-{unique}"))
    }

    #[test]
    fn parses_wit_into_selected_language_spec() {
        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

/// @nexus.endpoint "__temporal_system"
interface workflow-service {
  use nexus:temporal-types/model@1.0.0.{retry-policy, signal-function, workflow-function};

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_type"
    workflow: workflow-function,
    workflow-id: string,
    task-queue: string,
    /// @nexus.proto-field "signal_name"
    signal: signal-function,
    /// @nexus.source python="workflow.info().namespace" typescript="workflow.workflowInfo().namespace"
    namespace: option<string>,
  }

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
  record signal-with-start-workflow-execution-response {
    run-id: option<string>,
  }

  /// @nexus.output-transform
  ///   python-type="workflow.ExternalWorkflowHandle[typing.Any]"
  ///   python="workflow.get_external_workflow_handle(request.workflow_id, run_id=result.run_id)"
  ///   typescript-type="workflow.ExternalWorkflowHandle"
  ///   typescript="workflow.getExternalWorkflowHandle(request.workflowId, result.runId ?? undefined)"
  signal-with-start-workflow-execution: func(
    request: signal-with-start-workflow-execution-request
  ) -> signal-with-start-workflow-execution-response;
}
"#;

        let python = parse(Language::Python, wit);
        let typescript = parse(Language::TypeScript, wit);

        assert_eq!(python.support.fragments.len(), 1);
        assert_eq!(typescript.support.fragments.len(), 1);
        assert!(
            python.support.fragments[0]
                .path
                .ends_with("deps/nexus-temporal-types/python/model_overrides.py")
        );
        assert!(
            typescript.support.fragments[0]
                .path
                .ends_with("deps/nexus-temporal-types/typescript/model_overrides.ts")
        );
        assert!(
            python.support.fragments[0]
                .contents
                .contains("def retry_policy_from_proto(")
        );
        assert!(
            typescript.support.fragments[0]
                .contents
                .contains("export function retryPolicyFromProto(")
        );

        let request = python
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap();
        assert_eq!(
            python.services[0].operations[0].input_proto(),
            "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
        );
        assert_eq!(
            python.services[0].operations[0].output_proto(),
            "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
        );
        assert!(request.is_field_required("workflow_type"));
        assert!(request.is_field_hidden("header"));
        let model = request.generated_model().unwrap();
        assert_eq!(model.field_name_override("workflow_type"), Some("workflow"));
        assert_eq!(model.field_name_override("input"), Some("input"));
        assert_eq!(
            model.field_name_override("workflow_id"),
            Some("workflow-id")
        );
        assert!(model.function("workflow_type").unwrap().primary);
        assert_eq!(
            model.field_source("namespace"),
            Some("workflow.info().namespace")
        );

        let typescript_model = typescript
            .type_override(
                "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest",
            )
            .unwrap()
            .generated_model()
            .unwrap();
        assert!(typescript_model.function("workflow_type").is_some());
        assert!(typescript_model.with_arguments("signal_name").is_some());
        assert!(typescript_model.function("signal_name").is_none());
    }

    #[test]
    fn accumulates_builtin_and_input_support_fragments() {
        let temp_dir = unique_temp_dir("support-fragments");
        fs::create_dir_all(&temp_dir).unwrap();
        let input_path = temp_dir.join("input.wit");
        let extra_support_path = temp_dir.join("extra_support.py");
        fs::write(
            &extra_support_path,
            "def extra_support_hook() -> str:\n    return 'extra'\n",
        )
        .unwrap();

        let wit = r#"
/// @nexus.support python="extra_support.py"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

interface workflow-service {
  use nexus:temporal-types/model@1.0.0.{retry-policy};

  retry-policy-operation: func(request: retry-policy) -> retry-policy;
}
"#;

        let spec = ApiSpec::parse_for_language(Language::Python, wit, input_path).unwrap();
        assert_eq!(spec.support.fragments.len(), 2);
        assert!(
            spec.support.fragments[0]
                .path
                .ends_with("deps/nexus-temporal-types/python/model_overrides.py")
        );
        assert!(spec.support.fragments[1].path.ends_with("extra_support.py"));
        assert!(
            spec.support.fragments[0]
                .contents
                .contains("def retry_policy_from_proto(")
        );
        assert!(
            spec.support.fragments[1]
                .contents
                .contains("def extra_support_hook() -> str:")
        );

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn parses_sibling_wit_files_from_main_wit_package_directory() {
        let temp_dir = unique_temp_dir("sibling-wit");
        fs::create_dir_all(&temp_dir).unwrap();
        let shared_path = temp_dir.join("shared.wit");
        let input_path = temp_dir.join("main.wit");
        fs::write(
            &shared_path,
            r#"
package temporal:nexus@1.0.0;

interface shared {
  /// @nexus.proto "acme.foo.v1.LocalRetryPolicy"
  record local-retry-policy {
  }
}
"#,
        )
        .unwrap();

        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

interface workflow-service {
  use shared.{local-retry-policy};

  retry-policy-operation: func(request: local-retry-policy) -> local-retry-policy;
}
"#;

        let spec = ApiSpec::parse_for_language(Language::Python, wit, input_path).unwrap();
        assert_eq!(
            spec.services[0].operations[0].input_proto(),
            "acme.foo.v1.LocalRetryPolicy"
        );

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn ignores_sibling_wit_files_for_standalone_input_wit() {
        let temp_dir = unique_temp_dir("standalone-wit");
        fs::create_dir_all(&temp_dir).unwrap();
        let shared_path = temp_dir.join("shared.wit");
        let input_path = temp_dir.join("input.wit");
        fs::write(
            &shared_path,
            r#"
package temporal:nexus@1.0.0;

interface shared {
  /// @nexus.proto "acme.foo.v1.LocalRetryPolicy"
  record local-retry-policy {
  }
}
"#,
        )
        .unwrap();

        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

interface workflow-service {
  use nexus:temporal-types/model@1.0.0.{retry-policy};

  retry-policy-operation: func(request: retry-policy) -> retry-policy;
}
"#;

        let spec = ApiSpec::parse_for_language(Language::Python, wit, input_path).unwrap();
        assert_eq!(
            spec.services[0].operations[0].input_proto(),
            "temporal.api.common.v1.RetryPolicy"
        );

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn loads_builtin_wit_metadata_from_bundled_wit() {
        let builtins = load_builtin_wit_metadata().unwrap();

        let payload = builtins
            .proto_types
            .get("temporal.api.common.v1.Payload")
            .unwrap();
        assert_eq!(payload.wit_name, "payload");
        assert_eq!(payload.use_path, "nexus:temporal-types/model@1.0.0");

        let task_queue = builtins
            .proto_types
            .get("temporal.api.taskqueue.v1.TaskQueue")
            .unwrap();
        assert_eq!(task_queue.wit_name, "task-queue");
        assert_eq!(task_queue.use_path, "nexus:temporal-types/model@1.0.0");

        assert_eq!(
            builtins
                .type_use_paths
                .get("workflow-function")
                .map(String::as_str),
            Some("nexus:temporal-types/model@1.0.0")
        );
        assert_eq!(
            builtins
                .type_use_paths
                .get("signal-function")
                .map(String::as_str),
            Some("nexus:temporal-types/model@1.0.0")
        );
    }

    #[test]
    fn validates_wit_function_fields() {
        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

/// @nexus.endpoint "__temporal_system"
interface workflow-service {
  use nexus:temporal-types/model@1.0.0.{signal-function, workflow-function};

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_type"
    workflow: workflow-function,
    workflow-id: string,
    task-queue: string,
    /// @nexus.proto-field "signal_name"
    signal: signal-function,
  }

  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionResponse"
  record signal-with-start-workflow-execution-response {
    run-id: option<string>,
  }

  signal-with-start-workflow-execution: func(
    request: signal-with-start-workflow-execution-request
  ) -> signal-with-start-workflow-execution-response;
}
"#;

        validate(Language::Python, wit).unwrap();
    }

    #[test]
    fn parses_multiline_directive_arguments() {
        let directives = parse_directives(
            Some(
                r#"@nexus.type
  python="temporalio.common.RetryPolicy"
  typescript="common.RetryPolicy""#,
            ),
            &PathBuf::from("inline.wit"),
            "type `example`",
        )
        .unwrap();

        let directive = directive(
            &directives,
            "type",
            &PathBuf::from("inline.wit"),
            "type `example`",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            directive.value("python"),
            Some("temporalio.common.RetryPolicy")
        );
        assert_eq!(directive.value("typescript"), Some("common.RetryPolicy"));
    }

    #[test]
    fn rejects_duplicate_proto_field_mappings() {
        let wit = r#"
package temporal:nexus@1.0.0;

world system {
  export workflow-service;
}

interface workflow-service {
  /// @nexus.proto "temporal.api.workflowservice.v1.SignalWithStartWorkflowExecutionRequest"
  record signal-with-start-workflow-execution-request {
    /// @nexus.proto-field "workflow_id"
    workflow-id: string,
    /// @nexus.proto-field "workflow_id"
    workflow-id-alias: string,
  }
}
"#;

        let err = ApiSpec::parse_for_language(Language::Python, wit, PathBuf::from("inline.wit"))
            .unwrap_err();
        assert!(matches!(err, Error::InvalidWit { .. }));
    }
}
