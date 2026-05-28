use syn::visit::{self, Visit};

use super::builder::Builder;
use super::helpers::{expr_name, location, path_name};
use super::types::{PendingEdge, ResolutionStrategy};
use crate::model::EdgeKind;

pub struct FunctionCollector<'a> {
    pub builder: &'a mut Builder,
    pub owner: String,
    pub relative: String,
    pub source: &'a str,
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
            call_style: Some("direct".to_string()),
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
            call_style: Some("method".to_string()),
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
            call_style: Some("macro".to_string()),
        });
        visit::visit_macro(self, node);
    }
}
