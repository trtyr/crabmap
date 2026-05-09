use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodeGraph {
    pub schema_version: u32,
    pub project: Project,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub warnings: Vec<String>,
    pub semantic: Option<SemanticInfo>,
    pub mir: Option<MirInfo>,
    pub profiles: Vec<BuildProfile>,
    pub generated_at_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub root: String,
    pub workspace_root: String,
    pub packages: Vec<Package>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildProfile {
    pub name: String,
    pub features: Vec<String>,
    pub all_features: bool,
    pub no_default_features: bool,
    pub target: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub manifest_path: String,
    pub targets: Vec<Target>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SemanticInfo {
    pub provider: String,
    pub enabled: bool,
    pub scanned_symbols: usize,
    pub enriched_symbols: usize,
    pub confirmed_symbols: usize,
    pub enriched_edges: usize,
    pub confirmed_edges: usize,
    pub unresolved_items: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MirInfo {
    pub provider: String,
    pub enabled: bool,
    pub scanned_targets: usize,
    pub scanned_functions: usize,
    pub enriched_edges: usize,
    pub confirmed_edges: usize,
    pub unresolved_items: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub kind: Vec<String>,
    pub src_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub name: String,
    pub qualified_name: String,
    pub file: Option<String>,
    pub range: Option<Range>,
    pub visibility: Option<String>,
    pub signature: Option<String>,
    pub docs: Option<String>,
    pub metrics: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Project,
    Crate,
    File,
    Module,
    Function,
    Method,
    Constructor,
    Variable,
    Field,
    Property,
    Struct,
    Enum,
    EnumMember,
    Trait,
    Impl,
    TypeAlias,
    TypeParameter,
    Const,
    Static,
    Macro,
    Unknown,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Crate => "crate",
            Self::File => "file",
            Self::Module => "module",
            Self::Function => "function",
            Self::Method => "method",
            Self::Constructor => "constructor",
            Self::Variable => "variable",
            Self::Field => "field",
            Self::Property => "property",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::EnumMember => "enum_member",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::TypeAlias => "type_alias",
            Self::TypeParameter => "type_parameter",
            Self::Const => "const",
            Self::Static => "static",
            Self::Macro => "macro",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
    pub label: Option<String>,
    pub evidence: Option<Location>,
    pub weight: usize,
    pub source: EdgeSource,
    pub certainty: EdgeCertainty,
    pub profiles: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    Declares,
    Imports,
    Calls,
    Implements,
    HasMethod,
    UsesType,
    Returns,
    ModuleFile,
    PossibleDispatch,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Declares => "declares",
            Self::Imports => "imports",
            Self::Calls => "calls",
            Self::Implements => "implements",
            Self::HasMethod => "has_method",
            Self::UsesType => "uses_type",
            Self::Returns => "returns",
            Self::ModuleFile => "module_file",
            Self::PossibleDispatch => "possible_dispatch",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeSource {
    Ast,
    RustAnalyzer,
    Mir,
    Inferred,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeCertainty {
    Definite,
    Confirmed,
    Inferred,
    Possible,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Range {
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphStats {
    pub nodes: usize,
    pub edges: usize,
    pub by_kind: BTreeMap<String, usize>,
    pub by_edge: BTreeMap<String, usize>,
    pub by_source: BTreeMap<String, usize>,
    pub by_certainty: BTreeMap<String, usize>,
    pub files: usize,
    pub symbols: usize,
    pub warnings: usize,
    pub semantic: Option<SemanticInfo>,
    pub mir: Option<MirInfo>,
}

impl CodeGraph {
    pub fn stats(&self) -> GraphStats {
        let mut by_kind = BTreeMap::new();
        let mut by_edge = BTreeMap::new();
        let mut by_source = BTreeMap::new();
        let mut by_certainty = BTreeMap::new();
        for node in &self.nodes {
            *by_kind.entry(node.kind.as_str().to_string()).or_insert(0) += 1;
        }
        for edge in &self.edges {
            *by_edge.entry(edge.kind.as_str().to_string()).or_insert(0) += 1;
            *by_source
                .entry(edge.source.as_str().to_string())
                .or_insert(0) += 1;
            *by_certainty
                .entry(edge.certainty.as_str().to_string())
                .or_insert(0) += 1;
        }
        GraphStats {
            nodes: self.nodes.len(),
            edges: self.edges.len(),
            by_kind,
            by_edge,
            by_source,
            by_certainty,
            files: self
                .nodes
                .iter()
                .filter(|node| node.kind == NodeKind::File)
                .count(),
            symbols: self
                .nodes
                .iter()
                .filter(|node| {
                    !matches!(
                        node.kind,
                        NodeKind::Project | NodeKind::Crate | NodeKind::File
                    )
                })
                .count(),
            warnings: self.warnings.len(),
            semantic: self.semantic.clone(),
            mir: self.mir.clone(),
        }
    }
}

impl EdgeSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ast => "ast",
            Self::RustAnalyzer => "rust_analyzer",
            Self::Mir => "mir",
            Self::Inferred => "inferred",
        }
    }
}

impl EdgeCertainty {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Definite => "definite",
            Self::Confirmed => "confirmed",
            Self::Inferred => "inferred",
            Self::Possible => "possible",
        }
    }
}
