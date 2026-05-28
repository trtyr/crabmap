use crate::model::{EdgeKind, Location, NodeKind, Range};
use std::collections::BTreeMap;

pub struct PendingEdge {
    pub from: String,
    pub target: String,
    pub kind: EdgeKind,
    pub label: Option<String>,
    pub evidence: Option<Location>,
    pub source_file: Option<String>,
    pub resolution: ResolutionStrategy,
}

#[derive(Clone, Copy)]
pub enum ResolutionStrategy {
    Any,
    Callable,
    MethodOnly,
    MacroOnly,
}

pub struct NodeInput {
    pub kind: NodeKind,
    pub name: String,
    pub qualified_name: String,
    pub file: Option<String>,
    pub range: Option<Range>,
    pub visibility: Option<String>,
    pub signature: Option<String>,
    pub docs: Vec<String>,
    pub metrics: BTreeMap<String, usize>,
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
