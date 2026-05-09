use crate::model::{
    BuildProfile, CodeGraph, Edge, EdgeCertainty, EdgeKind, EdgeSource, Location, Node, NodeKind,
    Package, Project, Range, Target,
};
use anyhow::{Context, Result};
use cargo_metadata::{CargoOpt, MetadataCommand};
use ignore::WalkBuilder;
use quote::ToTokens;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

struct PendingEdge {
    from: String,
    target: String,
    kind: EdgeKind,
    label: Option<String>,
    evidence: Option<Location>,
    source_file: Option<String>,
    resolution: ResolutionStrategy,
}

#[derive(Clone, Copy)]
enum ResolutionStrategy {
    Any,
    Callable,
    MethodOnly,
    MacroOnly,
}

struct Builder {
    root: PathBuf,
    profile: String,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    pending: Vec<PendingEdge>,
    warnings: Vec<String>,
    ids: BTreeSet<String>,
    by_qname: HashMap<String, String>,
    by_name: HashMap<String, Vec<String>>,
}

pub struct IndexOptions {
    pub include_tests: bool,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub target: Option<String>,
    pub profile: String,
    pub on_progress: Option<std::sync::Arc<dyn Fn(usize, usize) + Send + Sync>>,
}

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
            if let Err(error) =
                index_file(&mut builder, &crate_id, &package.name, &src_path, &file)
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
    let trait_name = item.trait_.as_ref().map(|item| path_name(&item.1));
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

struct FunctionCollector<'a> {
    builder: &'a mut Builder,
    owner: String,
    relative: String,
    source: &'a str,
}

impl<'ast> Visit<'ast> for FunctionCollector<'_> {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        self.builder.pending.push(PendingEdge {
            from: self.owner.clone(),
            target: expr_name(&node.func),
            kind: EdgeKind::Calls,
            label: Some(expr_name(&node.func)),
            evidence: Some(location(
                &self.relative,
                self.source,
                &expr_name(&node.func),
            )),
            source_file: Some(self.relative.clone()),
            resolution: ResolutionStrategy::Callable,
        });
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.builder.pending.push(PendingEdge {
            from: self.owner.clone(),
            target: node.method.to_string(),
            kind: EdgeKind::Calls,
            label: Some(node.method.to_string()),
            evidence: Some(location(
                &self.relative,
                self.source,
                &node.method.to_string(),
            )),
            source_file: Some(self.relative.clone()),
            resolution: ResolutionStrategy::MethodOnly,
        });
        visit::visit_expr_method_call(self, node);
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.builder.pending.push(PendingEdge {
            from: self.owner.clone(),
            target: path_name(&node.path),
            kind: EdgeKind::Calls,
            label: Some(path_name(&node.path)),
            evidence: Some(location(
                &self.relative,
                self.source,
                &path_name(&node.path),
            )),
            source_file: Some(self.relative.clone()),
            resolution: ResolutionStrategy::MacroOnly,
        });
        visit::visit_macro(self, node);
    }
}

struct NodeInput {
    kind: NodeKind,
    name: String,
    qualified_name: String,
    file: Option<String>,
    range: Option<Range>,
    visibility: Option<String>,
    signature: Option<String>,
    docs: Vec<String>,
    metrics: BTreeMap<String, usize>,
}

impl Builder {
    fn new(root: PathBuf, profile: String) -> Self {
        Self {
            root,
            profile,
            nodes: Vec::new(),
            edges: Vec::new(),
            pending: Vec::new(),
            warnings: Vec::new(),
            ids: BTreeSet::new(),
            by_qname: HashMap::new(),
            by_name: HashMap::new(),
        }
    }

    fn add_node(&mut self, input: NodeInput) -> String {
        let base = format!(
            "{}:{}",
            input.kind.as_str(),
            input.qualified_name.replace([' ', '\n', '\t'], "_")
        );
        let id = self.unique_id(base);
        self.by_qname
            .insert(input.qualified_name.clone(), id.clone());
        self.by_name
            .entry(input.name.clone())
            .or_default()
            .push(id.clone());
        self.nodes.push(Node {
            id: id.clone(),
            kind: input.kind,
            name: input.name,
            qualified_name: input.qualified_name,
            file: input.file,
            range: input.range,
            visibility: input.visibility,
            signature: input.signature.map(compact),
            docs: if input.docs.is_empty() {
                None
            } else {
                Some(input.docs.join("\n"))
            },
            metrics: input.metrics,
        });
        id
    }

    fn symbol(
        &mut self,
        kind: NodeKind,
        name: String,
        qualified_name: String,
        relative: &str,
        source: &str,
        signature: Option<String>,
        visibility: Option<String>,
        docs: Vec<String>,
    ) -> String {
        let line = find_line(source, &name);
        self.add_node(NodeInput {
            kind,
            name,
            qualified_name,
            file: Some(relative.to_string()),
            range: Some(Range {
                start_line: line,
                end_line: line,
            }),
            visibility,
            signature,
            docs,
            metrics: BTreeMap::new(),
        })
    }

    fn symbol_at_line(
        &mut self,
        kind: NodeKind,
        name: String,
        qualified_name: String,
        relative: &str,
        line: usize,
        signature: Option<String>,
        visibility: Option<String>,
        docs: Vec<String>,
    ) -> String {
        self.add_node(NodeInput {
            kind,
            name,
            qualified_name,
            file: Some(relative.to_string()),
            range: Some(Range {
                start_line: line,
                end_line: line,
            }),
            visibility,
            signature,
            docs,
            metrics: BTreeMap::new(),
        })
    }

    fn edge(
        &mut self,
        from: &str,
        to: &str,
        kind: EdgeKind,
        label: Option<String>,
        evidence: Option<Location>,
    ) {
        self.edge_with_source(
            from,
            to,
            kind,
            label,
            evidence,
            EdgeSource::Ast,
            EdgeCertainty::Definite,
        )
    }

    fn edge_with_source(
        &mut self,
        from: &str,
        to: &str,
        kind: EdgeKind,
        label: Option<String>,
        evidence: Option<Location>,
        source: EdgeSource,
        certainty: EdgeCertainty,
    ) {
        if from == to {
            return;
        }
        if let Some(edge) = self
            .edges
            .iter_mut()
            .find(|edge| edge.from == from && edge.to == to && edge.kind == kind)
        {
            edge.weight += 1;
            if source == EdgeSource::RustAnalyzer {
                edge.source = EdgeSource::RustAnalyzer;
                edge.certainty = EdgeCertainty::Confirmed;
            }
            if !edge.profiles.iter().any(|profile| profile == &self.profile) {
                edge.profiles.push(self.profile.clone());
            }
            return;
        }
        self.edges.push(Edge {
            from: from.to_string(),
            to: to.to_string(),
            kind,
            label,
            evidence,
            weight: 1,
            source,
            certainty,
            profiles: vec![self.profile.clone()],
        });
    }

    fn type_use(
        &mut self,
        from: &str,
        relative: &str,
        source: &str,
        ty: &syn::Type,
        kind: EdgeKind,
    ) {
        for name in type_names(ty) {
            self.pending.push(PendingEdge {
                from: from.to_string(),
                target: name.clone(),
                kind: kind.clone(),
                label: Some(name.clone()),
                evidence: Some(location(relative, source, &name)),
                source_file: Some(relative.to_string()),
                resolution: ResolutionStrategy::Any,
            });
        }
    }

    fn resolve_pending(&mut self) {
        for edge in std::mem::take(&mut self.pending) {
            if let Some(to) = self.resolve_pending_edge(&edge) {
                self.edge(&edge.from, &to, edge.kind, edge.label, edge.evidence);
            }
        }
    }

    fn resolve_pending_edge(&self, edge: &PendingEdge) -> Option<String> {
        self.resolve(&edge.target, edge.source_file.as_deref(), edge.resolution)
    }

    fn add_trait_dispatch_edges(&mut self) {
        let trait_methods = self
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::HasMethod)
            .filter_map(|edge| {
                let owner = self.nodes.iter().find(|node| node.id == edge.from)?;
                let method = self.nodes.iter().find(|node| node.id == edge.to)?;
                (owner.kind == NodeKind::Trait)
                    .then(|| (owner.id.clone(), method.name.clone(), method.id.clone()))
            })
            .collect::<Vec<_>>();
        let impl_methods = self
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::HasMethod)
            .filter_map(|edge| {
                let owner = self.nodes.iter().find(|node| node.id == edge.from)?;
                let method = self.nodes.iter().find(|node| node.id == edge.to)?;
                (owner.kind == NodeKind::Impl)
                    .then(|| (owner.id.clone(), method.name.clone(), method.id.clone()))
            })
            .collect::<Vec<_>>();
        let implements = self
            .edges
            .iter()
            .filter(|edge| edge.kind == EdgeKind::Implements)
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect::<Vec<_>>();
        for (impl_id, trait_id) in implements {
            for (_, trait_method_name, trait_method_id) in trait_methods
                .iter()
                .filter(|(candidate_trait_id, _, _)| candidate_trait_id == &trait_id)
            {
                for (_, _, impl_method_id) in
                    impl_methods
                        .iter()
                        .filter(|(candidate_impl_id, impl_method_name, _)| {
                            candidate_impl_id == &impl_id && impl_method_name == trait_method_name
                        })
                {
                    self.edge_with_source(
                        trait_method_id,
                        impl_method_id,
                        EdgeKind::PossibleDispatch,
                        Some(trait_method_name.clone()),
                        None,
                        EdgeSource::Inferred,
                        EdgeCertainty::Possible,
                    );
                }
            }
        }
    }

    fn resolve(
        &self,
        target: &str,
        source_file: Option<&str>,
        resolution: ResolutionStrategy,
    ) -> Option<String> {
        if let Some(id) = self.by_qname.get(target).filter(|id| {
            self.node(id)
                .is_some_and(|node| self.matches_resolution(node, resolution))
        }) {
            return Some(id.clone());
        }
        if target.contains("::") {
            if let Some(node) = self.nodes.iter().find(|node| {
                node.qualified_name.ends_with(&format!("::{target}"))
                    && self.matches_resolution(node, resolution)
            }) {
                return Some(node.id.clone());
            }
        }
        let short = target.split("::").last().unwrap_or(target);
        let candidates = self
            .by_name
            .get(short)
            .into_iter()
            .flatten()
            .filter_map(|id| self.node(id))
            .filter(|node| self.matches_resolution(node, resolution))
            .collect::<Vec<_>>();
        if matches!(resolution, ResolutionStrategy::Callable) {
            if let Some(source_file) = source_file {
                let same_file = candidates
                    .iter()
                    .copied()
                    .filter(|node| node.file.as_deref() == Some(source_file))
                    .collect::<Vec<_>>();
                if same_file.len() == 1 {
                    return Some(same_file[0].id.clone());
                }
            }
        }
        (candidates.len() == 1).then(|| candidates[0].id.clone())
    }

    fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    fn matches_resolution(&self, node: &Node, resolution: ResolutionStrategy) -> bool {
        match resolution {
            ResolutionStrategy::Any => true,
            ResolutionStrategy::Callable => matches!(
                node.kind,
                NodeKind::Function | NodeKind::Method | NodeKind::Macro | NodeKind::Constructor
            ),
            ResolutionStrategy::MethodOnly => node.kind == NodeKind::Method,
            ResolutionStrategy::MacroOnly => node.kind == NodeKind::Macro,
        }
    }

    fn relative(&self, path: &Path) -> String {
        let value = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        if value.is_empty() {
            return ".".to_string();
        }
        value
    }

    fn unique_id(&mut self, base: String) -> String {
        if self.ids.insert(base.clone()) {
            return base;
        }
        let mut index = 2;
        loop {
            let candidate = format!("{base}#{index}");
            if self.ids.insert(candidate.clone()) {
                return candidate;
            }
            index += 1;
        }
    }
}

fn module_name(crate_name: &str, target_root: &Path, file: &Path) -> String {
    let root = target_root.parent().unwrap_or(target_root);
    let relative = file.strip_prefix(root).unwrap_or(file);
    let mut parts = vec![crate_name.replace('-', "_")];
    for part in relative.components() {
        let value = part.as_os_str().to_string_lossy();
        if value == "src" || value == "lib.rs" || value == "main.rs" || value == "mod.rs" {
            continue;
        }
        parts.push(value.trim_end_matches(".rs").to_string());
    }
    parts.join("::")
}

fn file_metrics(source: &str) -> BTreeMap<String, usize> {
    let mut metrics = BTreeMap::new();
    metrics.insert("lines".to_string(), source.lines().count());
    metrics.insert(
        "non_empty_lines".to_string(),
        source
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
    );
    metrics.insert(
        "comment_lines".to_string(),
        source
            .lines()
            .filter(|line| line.trim_start().starts_with("//"))
            .count(),
    );
    metrics
}

fn visibility(vis: &syn::Visibility) -> Option<String> {
    match vis {
        syn::Visibility::Public(_) => Some("pub".to_string()),
        syn::Visibility::Restricted(value) => Some(value.to_token_stream().to_string()),
        syn::Visibility::Inherited => None,
    }
}

fn docs(attrs: &[syn::Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            syn::Meta::NameValue(value) => {
                if let syn::Expr::Lit(lit) = &value.value {
                    if let syn::Lit::Str(value) = &lit.lit {
                        return Some(value.value().trim().to_string());
                    }
                }
                None
            }
            _ => None,
        })
        .collect()
}

fn flatten_use_tree(tree: &syn::UseTree, prefix: Option<String>) -> Vec<String> {
    match tree {
        syn::UseTree::Path(path) => flatten_use_tree(
            &path.tree,
            Some(match prefix {
                Some(prefix) => format!("{prefix}::{}", path.ident),
                None => path.ident.to_string(),
            }),
        ),
        syn::UseTree::Name(name) => vec![match prefix {
            Some(prefix) => format!("{prefix}::{}", name.ident),
            None => name.ident.to_string(),
        }],
        syn::UseTree::Rename(name) => vec![match prefix {
            Some(prefix) => format!("{prefix}::{}", name.ident),
            None => name.ident.to_string(),
        }],
        syn::UseTree::Glob(_) => prefix.into_iter().collect(),
        syn::UseTree::Group(group) => group
            .items
            .iter()
            .flat_map(|item| flatten_use_tree(item, prefix.clone()))
            .collect(),
    }
}

fn type_names(ty: &syn::Type) -> Vec<String> {
    let mut visitor = TypeCollector::default();
    visitor.visit_type(ty);
    visitor.names
}

#[derive(Default)]
struct TypeCollector {
    names: Vec<String>,
}

impl<'ast> Visit<'ast> for TypeCollector {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        self.names.push(path_name(&node.path));
        visit::visit_type_path(self, node);
    }
}

fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path_name(&path.path),
        _ => compact(ty.to_token_stream().to_string()),
    }
}

fn path_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn expr_name(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Path(path) => path_name(&path.path),
        syn::Expr::Field(field) => field.member.to_token_stream().to_string(),
        _ => compact(expr.to_token_stream().to_string()),
    }
}

fn location(file: &str, source: &str, needle: &str) -> Location {
    Location {
        file: file.to_string(),
        line: find_line(source, needle),
    }
}

fn find_line(source: &str, needle: &str) -> usize {
    source
        .lines()
        .position(|line| line.contains(needle))
        .map(|line| line + 1)
        .unwrap_or(1)
}

fn find_line_after(source: &str, needle: &str, start_line: usize) -> usize {
    source
        .lines()
        .enumerate()
        .skip(start_line.saturating_sub(1))
        .find(|(_, line)| line.contains(needle))
        .map(|(index, _)| index + 1)
        .unwrap_or(start_line)
}

fn compact(value: String) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
