use crate::model::CodeGraph;
use serde_json::{Value, json};

pub fn tests(graph: &CodeGraph, symbol: Option<&str>, limit: usize) -> Value {
    let targets = graph
        .project
        .packages
        .iter()
        .flat_map(|package| {
            package.targets.iter().map(|target| {
                json!({
                    "package": package.name,
                    "name": target.name,
                    "kind": target.kind,
                    "src_path": target.src_path
                })
            })
        })
        .collect::<Vec<_>>();
    let mut tests = graph
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                crate::model::NodeKind::Function | crate::model::NodeKind::Method
            )
        })
        .filter(|node| {
            node.file
                .as_deref()
                .is_some_and(|file| file.contains("test"))
                || node.name.starts_with("test_")
                || node.name.ends_with("_test")
                || node.signature.as_deref().is_some_and(|signature| {
                    signature.contains("# [test]") || signature.contains("# [tokio :: test]")
                })
        })
        .filter(|node| {
            symbol.is_none_or(|symbol| {
                let haystack = format!(
                    "{} {} {}",
                    node.name,
                    node.qualified_name,
                    node.file.as_deref().unwrap_or_default()
                );
                haystack.contains(symbol)
            })
        })
        .map(|node| {
            json!({
                "kind": node.kind.as_str(),
                "name": node.name,
                "qualified_name": node.qualified_name,
                "file": node.file,
                "range": node.range
            })
        })
        .collect::<Vec<_>>();
    tests.truncate(limit);
    json!({
        "kind": "tests",
        "query": symbol,
        "targets": targets,
        "candidate_tests": tests,
        "note": "This is a conservative static hint based on test-like files/names/signatures and Cargo targets."
    })
}

pub fn changed_targets(graph: &CodeGraph, changed_files: &[String]) -> Value {
    let targets = graph
        .project
        .packages
        .iter()
        .flat_map(|package| {
            package.targets.iter().filter_map(|target| {
                changed_files
                    .iter()
                    .any(|file| target.src_path.ends_with(file) || file.starts_with("src/"))
                    .then(|| {
                        json!({
                            "package": package.name,
                            "target": target.name,
                            "kind": target.kind,
                            "src_path": target.src_path
                        })
                    })
            })
        })
        .collect::<Vec<_>>();
    json!({ "targets": targets })
}
