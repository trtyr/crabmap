use crate::model::{Edge, EdgeCertainty, EdgeKind, EdgeSource, Node, NodeKind};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::helpers::compact;
use super::types::{NodeInput, PendingEdge, ResolutionStrategy};

pub struct Builder {
    pub root: PathBuf,
    pub profile: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub pending: Vec<PendingEdge>,
    pub warnings: Vec<String>,
    ids: BTreeSet<String>,
    by_qname: HashMap<String, String>,
    by_name: HashMap<String, Vec<String>>,
}

impl Builder {
    pub fn new(root: PathBuf, profile: String) -> Self {
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

    pub fn add_node(&mut self, input: NodeInput) -> String {
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

    pub fn symbol(
        &mut self,
        kind: NodeKind,
        name: String,
        qualified_name: String,
        relative: &str,
        source: &str,
        signature: Option<String>,
        visibility: Option<String>,
        docs: Vec<String>,
        end_line: Option<usize>,
    ) -> String {
        let line = super::helpers::find_line(source, &name);
        self.add_node(NodeInput {
            kind,
            name,
            qualified_name,
            file: Some(relative.to_string()),
            range: Some(crate::model::Range {
                start_line: line,
                end_line: end_line.unwrap_or(line),
            }),
            visibility,
            signature,
            docs,
            metrics: std::collections::BTreeMap::new(),
        })
    }

    pub fn symbol_at_line(
        &mut self,
        kind: NodeKind,
        name: String,
        qualified_name: String,
        relative: &str,
        line: usize,
        signature: Option<String>,
        visibility: Option<String>,
        docs: Vec<String>,
        end_line: Option<usize>,
    ) -> String {
        self.add_node(NodeInput {
            kind,
            name,
            qualified_name,
            file: Some(relative.to_string()),
            range: Some(crate::model::Range {
                start_line: line,
                end_line: end_line.unwrap_or(line),
            }),
            visibility,
            signature,
            docs,
            metrics: std::collections::BTreeMap::new(),
        })
    }

    pub fn edge(
        &mut self,
        from: &str,
        to: &str,
        kind: EdgeKind,
        label: Option<String>,
        evidence: Option<crate::model::Location>,
        call_style: Option<String>,
    ) {
        self.edge_with_source(
            from,
            to,
            kind,
            label,
            evidence,
            EdgeSource::Ast,
            EdgeCertainty::Definite,
            call_style,
        )
    }

    pub fn edge_with_source(
        &mut self,
        from: &str,
        to: &str,
        kind: EdgeKind,
        label: Option<String>,
        evidence: Option<crate::model::Location>,
        source: EdgeSource,
        certainty: EdgeCertainty,
        call_style: Option<String>,
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
            call_style,
            profiles: vec![self.profile.clone()],
        });
    }

    pub fn type_use(
        &mut self,
        from: &str,
        relative: &str,
        source: &str,
        ty: &syn::Type,
        kind: EdgeKind,
    ) {
        for name in super::helpers::type_names(ty) {
            self.pending.push(PendingEdge {
                from: from.to_string(),
                target: name.clone(),
                kind: kind.clone(),
                label: Some(name.clone()),
                evidence: Some(super::helpers::location(relative, source, &name)),
                source_file: Some(relative.to_string()),
                resolution: ResolutionStrategy::Any,
                call_style: None,
            });
        }
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn relative(&self, path: &Path) -> String {
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

    pub fn unique_id(&mut self, base: String) -> String {
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

    pub fn resolve_pending(&mut self) {
        for edge in std::mem::take(&mut self.pending) {
            if let Some(to) = self.resolve_pending_edge(&edge) {
                self.edge(&edge.from, &to, edge.kind, edge.label, edge.evidence, edge.call_style.clone());
            }
        }
    }

    fn resolve_pending_edge(&self, edge: &PendingEdge) -> Option<String> {
        self.resolve(&edge.target, edge.source_file.as_deref(), edge.resolution)
    }

    pub fn add_trait_dispatch_edges(&mut self) {
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
                        None,
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
}
