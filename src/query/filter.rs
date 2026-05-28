use crate::model::{EdgeKind, Node, NodeKind};
use super::index::QueryIndex;

#[derive(Clone, Debug)]
pub struct SymbolFilter {
    pub visibility: Option<String>,
    pub no_docs: bool,
    pub dead: bool,
    pub test_only: bool,
    pub min_callers: Option<usize>,
    pub max_callers: Option<usize>,
    pub min_degree: Option<usize>,
    pub max_degree: Option<usize>,
}

impl Default for SymbolFilter {
    fn default() -> Self {
        Self {
            visibility: None,
            no_docs: false,
            dead: false,
            test_only: false,
            min_callers: None,
            max_callers: None,
            min_degree: None,
            max_degree: None,
        }
    }
}

impl SymbolFilter {
    pub fn matches(&self, node: &Node, index: &QueryIndex) -> bool {
        // Only filter function-like nodes for dead/test_only
        let is_infrastructure = !matches!(
            node.kind,
            NodeKind::Function
                | NodeKind::Method
                | NodeKind::Struct
                | NodeKind::Enum
                | NodeKind::Trait
        );

        if let Some(ref vis) = self.visibility {
            if node.visibility.as_deref() != Some(vis.as_str()) {
                return false;
            }
        }
        if self.no_docs && !node.docs.as_deref().is_some_and(|d| !d.trim().is_empty()) {
            // matches: no docs OR empty docs — keep going
        } else if self.no_docs {
            return false;
        }
        if (self.dead || self.test_only) && is_infrastructure {
            return false;
        }
        if self.dead || self.test_only {
            let caller_count: usize = index
                .edges(&node.id, false)
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .count();
            if self.dead && caller_count > 0 {
                return false;
            }
            if self.test_only && caller_count == 0 {
                return false;
            }
            if self.test_only && caller_count > 0 {
                let all_from_tests = index
                    .edges(&node.id, false)
                    .iter()
                    .filter(|e| e.kind == EdgeKind::Calls)
                    .all(|e| {
                        index.node(&e.from).map_or(false, |n| {
                            n.file
                                .as_deref()
                                .is_some_and(|f| f.contains("test"))
                                || n.name.starts_with("test_")
                                || n.name.ends_with("_test")
                        })
                    });
                if !all_from_tests {
                    return false;
                }
            }
        }
        if let Some(min) = self.min_callers {
            let count = index
                .edges(&node.id, false)
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .count();
            if count < min {
                return false;
            }
        }
        if let Some(max) = self.max_callers {
            let count = index
                .edges(&node.id, false)
                .iter()
                .filter(|e| e.kind == EdgeKind::Calls)
                .count();
            if count > max {
                return false;
            }
        }
        if let Some(min) = self.min_degree {
            if index.degree(&node.id) < min {
                return false;
            }
        }
        if let Some(max) = self.max_degree {
            if index.degree(&node.id) > max {
                return false;
            }
        }
        true
    }

    pub fn description(&self) -> Vec<String> {
        let mut desc = Vec::new();
        if let Some(ref vis) = self.visibility {
            desc.push(format!("visibility={vis}"));
        }
        if self.no_docs {
            desc.push("no-docs".into());
        }
        if self.dead {
            desc.push("dead".into());
        }
        if self.test_only {
            desc.push("test-only".into());
        }
        if let Some(n) = self.min_callers {
            desc.push(format!("min-callers>={n}"));
        }
        if let Some(n) = self.max_callers {
            desc.push(format!("max-callers<={n}"));
        }
        if let Some(n) = self.min_degree {
            desc.push(format!("min-degree>={n}"));
        }
        if let Some(n) = self.max_degree {
            desc.push(format!("max-degree<={n}"));
        }
        desc
    }
}
