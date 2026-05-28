use crate::model::Location;
use quote::ToTokens;
use std::collections::BTreeMap;
use syn::visit::Visit;

pub fn module_name(
    crate_name: &str,
    target_root: &std::path::Path,
    file: &std::path::Path,
) -> String {
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

pub fn file_metrics(source: &str) -> BTreeMap<String, usize> {
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

pub fn visibility(vis: &syn::Visibility) -> Option<String> {
    match vis {
        syn::Visibility::Public(_) => Some("pub".to_string()),
        syn::Visibility::Restricted(value) => Some(value.to_token_stream().to_string()),
        syn::Visibility::Inherited => None,
    }
}

pub fn docs(attrs: &[syn::Attribute]) -> Vec<String> {
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

pub fn flatten_use_tree(tree: &syn::UseTree, prefix: Option<String>) -> Vec<String> {
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

pub fn type_names(ty: &syn::Type) -> Vec<String> {
    let mut visitor = TypeCollector::default();
    visitor.visit_type(ty);
    visitor.names
}

#[derive(Default)]
pub struct TypeCollector {
    pub names: Vec<String>,
}

impl<'ast> syn::visit::Visit<'ast> for TypeCollector {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        self.names.push(path_name(&node.path));
        syn::visit::visit_type_path(self, node);
    }
}

pub fn type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(path) => path_name(&path.path),
        _ => compact(ty.to_token_stream().to_string()),
    }
}

pub fn path_name(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

pub fn expr_name(expr: &syn::Expr) -> String {
    match expr {
        syn::Expr::Path(path) => path_name(&path.path),
        syn::Expr::Field(field) => field.member.to_token_stream().to_string(),
        _ => compact(expr.to_token_stream().to_string()),
    }
}

pub fn location(file: &str, source: &str, needle: &str) -> Location {
    Location {
        file: file.to_string(),
        line: find_line(source, needle),
    }
}

pub fn find_line(source: &str, needle: &str) -> usize {
    source
        .lines()
        .position(|line| line.contains(needle))
        .map(|line| line + 1)
        .unwrap_or(1)
}

pub fn find_item_end_line(source: &str, start_line: usize) -> usize {
    let mut depth: i32 = 0;
    let mut found_open = false;
    for (i, line) in source.lines().enumerate() {
        let line_num = i + 1;
        if line_num < start_line {
            continue;
        }
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    found_open = true;
                }
                '}' => {
                    depth -= 1;
                    if found_open && depth == 0 {
                        return line_num;
                    }
                }
                _ => {}
            }
        }
    }
    start_line // fallback: single-line item or no braces
}

pub fn find_line_after(source: &str, needle: &str, start_line: usize) -> usize {
    source
        .lines()
        .enumerate()
        .skip(start_line.saturating_sub(1))
        .find(|(_, line)| line.contains(needle))
        .map(|(index, _)| index + 1)
        .unwrap_or(start_line)
}

pub fn compact(value: String) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
