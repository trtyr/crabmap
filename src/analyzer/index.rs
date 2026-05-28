use crate::model::{
    BuildProfile, CodeGraph, EdgeKind, Location, NodeKind, Package, Project, Range, Target,
};
use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, MetadataCommand};
use ignore::WalkBuilder;
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use syn::visit::Visit;

use super::builder::Builder;
use super::helpers::{
    docs, file_metrics, find_line, find_line_after, flatten_use_tree, location, module_name,
    type_name, visibility,
};
use super::types::{IndexOptions, NodeInput, PendingEdge, ResolutionStrategy};
use super::visitors::FunctionCollector;

pub fn index_project(project: &Path, options: IndexOptions) -> Result<CodeGraph> {
    let root = project
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", project.display()))?;
    let mut metadata_command = MetadataCommand::new();
    metadata_command.manifest_path(find_manifest(&root)?);
    if options.all_features {
        metadata_command.features(CargoOpt::AllFeatures);
    }
    if options.no_default_features {
        metadata_command.features(CargoOpt::NoDefaultFeatures);
    }
    if !options.features.is_empty() {
        metadata_command.features(CargoOpt::SomeFeatures(options.features.clone()));
    }
    if let Some(target) = &options.target {
        metadata_command.other_options(vec!["--filter-platform".to_string(), target.clone()]);
    }
    let metadata = metadata_command
        .exec()
        .context("failed to run cargo metadata")?;
    let workspace_root = PathBuf::from(metadata.workspace_root.as_std_path());
    let mut builder = Builder::new(workspace_root.clone(), options.profile.clone());
    let project_id = builder.add_node(NodeInput {
        kind: NodeKind::Project,
        name: workspace_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string(),
        qualified_name: workspace_root.display().to_string(),
        file: None,
        range: None,
        visibility: None,
        signature: None,
        docs: Vec::new(),
        metrics: BTreeMap::new(),
    });
    let mut packages = Vec::new();
    for package in metadata.packages {
        if !metadata.workspace_members.contains(&package.id) {
            continue;
        }
        let crate_id = builder.add_node(NodeInput {
            kind: NodeKind::Crate,
            name: package.name.to_string(),
            qualified_name: package.name.to_string(),
            file: Some(builder.relative(package.manifest_path.as_std_path())),
            range: None,
            visibility: None,
            signature: None,
            docs: package
                .description
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
            metrics: BTreeMap::new(),
        });
        builder.edge(&project_id, &crate_id, EdgeKind::Contains, None, None);
        let mut targets = Vec::new();
        let mut indexed_files = BTreeSet::new();
        // Phase 1: collect files
        let mut files_to_index: Vec<(PathBuf, PathBuf)> = Vec::new();
        for target in package.targets {
            let src_path = PathBuf::from(target.src_path.as_std_path());
            if !src_path.exists() {
                continue;
            }
            targets.push(Target {
                name: target.name.to_string(),
                kind: target.kind.iter().map(ToString::to_string).collect(),
                src_path: builder.relative(&src_path),
            });
            for file in rust_files(&src_path, options.include_tests)? {
                if !indexed_files.insert(file.clone()) {
                    continue;
                }
                files_to_index.push((file, src_path.clone()));
            }
        }
        // Phase 2: index with progress
        let total = files_to_index.len();
        for (idx, (file, src_path)) in files_to_index.into_iter().enumerate() {
            if let Some(ref cb) = options.on_progress {
                cb(idx + 1, total);
            }
            if let Err(error) = index_file(&mut builder, &crate_id, &package.name, &src_path, &file)
            {
                builder
                    .warnings
                    .push(format!("{}: {error:#}", file.display()));
            }
        }
        packages.push(Package {
            name: package.name.to_string(),
            manifest_path: builder.relative(package.manifest_path.as_std_path()),
            targets,
        });
    }
    builder.resolve_pending();
    builder.add_trait_dispatch_edges();
    Ok(CodeGraph {
        schema_version: 2,
        project: Project {
            root: builder.relative(&root),
            workspace_root: builder.relative(&workspace_root),
            packages,
        },
        nodes: builder.nodes,
        edges: builder.edges,
        warnings: builder.warnings,
        semantic: None,
        mir: None,
        profiles: vec![BuildProfile {
            name: options.profile,
            features: options.features,
            all_features: options.all_features,
            no_default_features: options.no_default_features,
            target: options.target,
        }],
        generated_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    })
}

fn find_manifest(root: &Path) -> Result<PathBuf> {
    if root.join("Cargo.toml").exists() {
        return Ok(root.join("Cargo.toml"));
    }
    for ancestor in root.ancestors() {
        if ancestor.join("Cargo.toml").exists() {
            return Ok(ancestor.join("Cargo.toml"));
        }
    }
    anyhow::bail!("Cargo.toml not found from {}", root.display())
}

fn rust_files(src_path: &Path, include_tests: bool) -> Result<Vec<PathBuf>> {
    let root = if src_path.is_file() {
        src_path.parent().unwrap_or(src_path)
    } else {
        src_path
    };
    let files = WalkBuilder::new(root)
        .hidden(false)
        .build()
        .filter_map(Result::ok)
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("rs"))
        .filter(|path| include_tests || !path.components().any(|part| part.as_os_str() == "tests"))
        .collect();
    Ok(files)
}

fn index_file(
    builder: &mut Builder,
    crate_id: &str,
    crate_name: &str,
    target_root: &Path,
    file: &Path,
) -> Result<()> {
    let source = std::fs::read_to_string(file)
        .with_context(|| format!("failed to read {}", file.display()))?;
    let syntax =
        syn::parse_file(&source).with_context(|| format!("failed to parse {}", file.display()))?;
    let relative = builder.relative(file);
    let module_name = module_name(crate_name, target_root, file);
    let file_id = builder.add_node(NodeInput {
        kind: NodeKind::File,
        name: relative.clone(),
        qualified_name: relative.clone(),
        file: Some(relative.clone()),
        range: Some(Range {
            start_line: 1,
            end_line: source.lines().count().max(1),
        }),
        visibility: None,
        signature: None,
        docs: Vec::new(),
        metrics: file_metrics(&source),
    });
    let module_id = builder.add_node(NodeInput {
        kind: NodeKind::Module,
        name: module_name.clone(),
        qualified_name: module_name.clone(),
        file: Some(relative.clone()),
        range: None,
        visibility: None,
        signature: None,
        docs: Vec::new(),
        metrics: BTreeMap::new(),
    });
    builder.edge(crate_id, &file_id, EdgeKind::Contains, None, None);
    builder.edge(&file_id, &module_id, EdgeKind::ModuleFile, None, None);
    for item in &syntax.items {
        index_item(
            builder,
            &file_id,
            &module_id,
            &module_name,
            &relative,
            &source,
            item,
        );
    }
    Ok(())
}

fn index_item(
    builder: &mut Builder,
    file_id: &str,
    owner_id: &str,
    module_name: &str,
    relative: &str,
    source: &str,
    item: &syn::Item,
) {
    match item {
        syn::Item::Fn(item) => {
            let qname = format!("{module_name}::{}", item.sig.ident);
            let id = builder.symbol(
                NodeKind::Function,
                item.sig.ident.to_string(),
                qname,
                relative,
                source,
                Some(item.sig.to_token_stream().to_string()),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.sig.ident.to_string())),
            );
            collect_function_edges(builder, &id, relative, source, &item.sig, &item.block);
        }
        syn::Item::Struct(item) => {
            let id = builder.symbol(
                NodeKind::Struct,
                item.ident.to_string(),
                format!("{module_name}::{}", item.ident),
                relative,
                source,
                Some(item.to_token_stream().to_string()),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
            for field in &item.fields {
                if let Some(ident) = &field.ident {
                    let field_id = builder.symbol(
                        NodeKind::Field,
                        ident.to_string(),
                        format!("{module_name}::{}::{ident}", item.ident),
                        relative,
                        source,
                        Some(field.ty.to_token_stream().to_string()),
                        visibility(&field.vis),
                        docs(&field.attrs),
                    );
                    builder.edge(
                        &id,
                        &field_id,
                        EdgeKind::Declares,
                        None,
                        Some(location(relative, source, &ident.to_string())),
                    );
                    builder.type_use(&field_id, relative, source, &field.ty, EdgeKind::UsesType);
                    continue;
                }
                builder.type_use(&id, relative, source, &field.ty, EdgeKind::UsesType);
            }
        }
        syn::Item::Enum(item) => {
            let enum_line = find_line(source, &item.ident.to_string());
            let id = builder.symbol(
                NodeKind::Enum,
                item.ident.to_string(),
                format!("{module_name}::{}", item.ident),
                relative,
                source,
                Some(format!("enum {}", item.ident)),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
            for variant in &item.variants {
                let variant_name = variant.ident.to_string();
                let variant_line = find_line_after(source, &variant_name, enum_line);
                let variant_id = builder.symbol_at_line(
                    NodeKind::EnumMember,
                    variant_name.clone(),
                    format!("{module_name}::{}::{}", item.ident, variant.ident),
                    relative,
                    variant_line,
                    Some(variant.to_token_stream().to_string()),
                    None,
                    docs(&variant.attrs),
                );
                builder.edge(
                    &id,
                    &variant_id,
                    EdgeKind::Declares,
                    None,
                    Some(Location {
                        file: relative.to_string(),
                        line: variant_line,
                    }),
                );
                for field in &variant.fields {
                    builder.type_use(&variant_id, relative, source, &field.ty, EdgeKind::UsesType);
                }
            }
        }
        syn::Item::Trait(item) => {
            let trait_id = builder.symbol(
                NodeKind::Trait,
                item.ident.to_string(),
                format!("{module_name}::{}", item.ident),
                relative,
                source,
                Some(format!("trait {}", item.ident)),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &trait_id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
            for trait_item in &item.items {
                if let syn::TraitItem::Fn(method) = trait_item {
                    let id = builder.symbol(
                        NodeKind::Method,
                        method.sig.ident.to_string(),
                        format!("{module_name}::{}::{}", item.ident, method.sig.ident),
                        relative,
                        source,
                        Some(method.sig.to_token_stream().to_string()),
                        None,
                        docs(&method.attrs),
                    );
                    builder.edge(
                        &trait_id,
                        &id,
                        EdgeKind::HasMethod,
                        None,
                        Some(location(relative, source, &method.sig.ident.to_string())),
                    );
                    for input in &method.sig.inputs {
                        if let syn::FnArg::Typed(input) = input {
                            builder.type_use(&id, relative, source, &input.ty, EdgeKind::UsesType);
                        }
                    }
                }
            }
        }
        syn::Item::Impl(item) => index_impl(builder, file_id, module_name, relative, source, item),
        syn::Item::Use(item) => {
            for imported in flatten_use_tree(&item.tree, None) {
                builder.pending.push(PendingEdge {
                    from: owner_id.to_string(),
                    target: imported.clone(),
                    kind: EdgeKind::Imports,
                    label: Some(imported),
                    evidence: Some(location(relative, source, "use")),
                    source_file: Some(relative.to_string()),
                    resolution: ResolutionStrategy::Any,
                });
            }
        }
        syn::Item::Mod(item) => {
            let qname = format!("{module_name}::{}", item.ident);
            let id = builder.symbol(
                NodeKind::Module,
                item.ident.to_string(),
                qname.clone(),
                relative,
                source,
                Some(format!("mod {}", item.ident)),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
            if let Some((_, items)) = &item.content {
                for nested in items {
                    index_item(builder, file_id, &id, &qname, relative, source, nested);
                }
            }
        }
        syn::Item::Type(item) => {
            let id = builder.symbol(
                NodeKind::TypeAlias,
                item.ident.to_string(),
                format!("{module_name}::{}", item.ident),
                relative,
                source,
                Some(item.to_token_stream().to_string()),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
            builder.type_use(&id, relative, source, &item.ty, EdgeKind::UsesType);
        }
        syn::Item::Const(item) => {
            let id = builder.symbol(
                NodeKind::Const,
                item.ident.to_string(),
                format!("{module_name}::{}", item.ident),
                relative,
                source,
                Some(item.to_token_stream().to_string()),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
        }
        syn::Item::Static(item) => {
            let id = builder.symbol(
                NodeKind::Static,
                item.ident.to_string(),
                format!("{module_name}::{}", item.ident),
                relative,
                source,
                Some(item.to_token_stream().to_string()),
                visibility(&item.vis),
                docs(&item.attrs),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &item.ident.to_string())),
            );
        }
        syn::Item::Macro(item) => {
            let name = item
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| item.mac.path.to_token_stream().to_string());
            let id = builder.symbol(
                NodeKind::Macro,
                name.clone(),
                format!("{module_name}::{name}"),
                relative,
                source,
                Some(item.to_token_stream().to_string()),
                None,
                Vec::new(),
            );
            builder.edge(
                owner_id,
                &id,
                EdgeKind::Declares,
                None,
                Some(location(relative, source, &name)),
            );
        }
        _ => {}
    }
}

fn index_impl(
    builder: &mut Builder,
    file_id: &str,
    module_name: &str,
    relative: &str,
    source: &str,
    item: &syn::ItemImpl,
) {
    let self_ty = type_name(&item.self_ty);
    let trait_name = item
        .trait_
        .as_ref()
        .map(|item| super::helpers::path_name(&item.1));
    let impl_name = trait_name
        .as_ref()
        .map(|name| format!("impl {name} for {self_ty}"))
        .unwrap_or_else(|| format!("impl {self_ty}"));
    let impl_id = builder.symbol(
        NodeKind::Impl,
        impl_name.clone(),
        format!("{module_name}::{impl_name}"),
        relative,
        source,
        Some(item.to_token_stream().to_string()),
        None,
        Vec::new(),
    );
    builder.edge(
        file_id,
        &impl_id,
        EdgeKind::Declares,
        None,
        Some(location(relative, source, "impl")),
    );
    builder.pending.push(PendingEdge {
        from: impl_id.clone(),
        target: self_ty.clone(),
        kind: EdgeKind::UsesType,
        label: Some(self_ty.clone()),
        evidence: Some(location(relative, source, &self_ty)),
        source_file: Some(relative.to_string()),
        resolution: ResolutionStrategy::Any,
    });
    if let Some(trait_name) = trait_name {
        builder.pending.push(PendingEdge {
            from: impl_id.clone(),
            target: trait_name.clone(),
            kind: EdgeKind::Implements,
            label: Some(trait_name),
            evidence: Some(location(relative, source, "impl")),
            source_file: Some(relative.to_string()),
            resolution: ResolutionStrategy::Any,
        });
    }
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            let id = builder.symbol(
                NodeKind::Method,
                method.sig.ident.to_string(),
                format!("{module_name}::{self_ty}::{}", method.sig.ident),
                relative,
                source,
                Some(method.sig.to_token_stream().to_string()),
                visibility(&method.vis),
                docs(&method.attrs),
            );
            builder.edge(
                &impl_id,
                &id,
                EdgeKind::HasMethod,
                None,
                Some(location(relative, source, &method.sig.ident.to_string())),
            );
            collect_function_edges(builder, &id, relative, source, &method.sig, &method.block);
        }
    }
}

fn collect_function_edges(
    builder: &mut Builder,
    owner: &str,
    relative: &str,
    source: &str,
    sig: &syn::Signature,
    block: &syn::Block,
) {
    for input in &sig.inputs {
        if let syn::FnArg::Typed(input) = input {
            builder.type_use(owner, relative, source, &input.ty, EdgeKind::UsesType);
        }
    }
    if let syn::ReturnType::Type(_, ty) = &sig.output {
        builder.type_use(owner, relative, source, ty, EdgeKind::Returns);
    }
    let mut collector = FunctionCollector {
        builder,
        owner: owner.to_string(),
        relative: relative.to_string(),
        source,
    };
    collector.visit_block(block);
}
