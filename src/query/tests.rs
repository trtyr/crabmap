use super::commands::{file, module, neighbors, symbol};
use super::traversal::path;
use crate::model::{
    BuildProfile, CodeGraph, Edge, EdgeCertainty, EdgeKind, EdgeSource, Node, NodeKind, Package,
    Project, Range, Target,
};
use std::collections::BTreeMap;

#[test]
fn symbol_reports_ambiguous_short_names() {
    let graph = ambiguous_graph();

    let result = symbol(&graph, "index_project").unwrap();

    assert_eq!(result["kind"], "ambiguous");
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert!(
        matches
            .iter()
            .any(|node| node["qualified_name"] == "crabmap::index_project")
    );
    assert!(
        matches
            .iter()
            .any(|node| node["qualified_name"] == "crabmap::analyzer::index_project")
    );
}

#[test]
fn symbol_prefers_exact_qualified_name_match() {
    let graph = ambiguous_graph();

    let result = symbol(&graph, "crabmap::index_project").unwrap();

    assert_eq!(result["kind"], "symbol");
    assert_eq!(result["node"]["qualified_name"], "crabmap::index_project");
}

#[test]
fn neighbors_fail_on_ambiguous_short_names() {
    let graph = ambiguous_graph();

    let error = neighbors(&graph, "index_project", "calls", true, 1, 10).unwrap_err();

    assert_eq!(
        error.to_string(),
        "symbol `index_project` is ambiguous, matches: crabmap::index_project, crabmap::analyzer::index_project"
    );
}

#[test]
fn path_fails_when_source_symbol_is_ambiguous() {
    let graph = ambiguous_graph();

    let error = path(&graph, "index_project", "helper", 3).unwrap_err();

    assert_eq!(
        error.to_string(),
        "source `index_project` is ambiguous, matches: crabmap::index_project, crabmap::analyzer::index_project"
    );
}

#[test]
fn file_lists_symbols_declared_in_matching_path() {
    let graph = file_and_module_graph();

    let result = file(&graph, "src/main.rs").unwrap();
    let declares = result["declares"].as_array().unwrap();

    assert_eq!(declares.len(), 3);
    assert!(
        declares
            .iter()
            .any(|item| item["node"]["qualified_name"] == "crabmap::model")
    );
    assert!(
        declares
            .iter()
            .any(|item| item["node"]["qualified_name"] == "crabmap::main_fn")
    );
    assert!(
        declares
            .iter()
            .any(|item| item["node"]["qualified_name"] == "crabmap::App")
    );
}

#[test]
fn module_prefers_declaring_module_node_with_matching_qualified_name() {
    let graph = file_and_module_graph();

    let result = module(&graph, "crabmap::model").unwrap();
    let declares = result["declares"].as_array().unwrap();

    assert_eq!(result["node"]["id"], "module:crabmap::model#2");
    assert_eq!(declares.len(), 1);
    assert_eq!(
        declares[0]["node"]["qualified_name"],
        "crabmap::model::Node"
    );
}

fn ambiguous_graph() -> CodeGraph {
    CodeGraph {
        schema_version: 1,
        project: project(),
        nodes: vec![
            node(
                "function:crabmap::index_project",
                NodeKind::Function,
                "index_project",
                "crabmap::index_project",
                "src/main.rs",
                10,
            ),
            node(
                "function:crabmap::analyzer::index_project",
                NodeKind::Function,
                "index_project",
                "crabmap::analyzer::index_project",
                "src/analyzer.rs",
                30,
            ),
            node(
                "function:crabmap::helper",
                NodeKind::Function,
                "helper",
                "crabmap::helper",
                "src/main.rs",
                50,
            ),
        ],
        edges: vec![Edge {
            from: "function:crabmap::index_project".to_string(),
            to: "function:crabmap::helper".to_string(),
            kind: EdgeKind::Calls,
            label: None,
            evidence: None,
            weight: 1,
            source: EdgeSource::Ast,
            certainty: EdgeCertainty::Definite,
            profiles: vec![],
        }],
        warnings: vec![],
        semantic: None,
        mir: None,
        profiles: vec![profile()],
        generated_at_ms: 0,
    }
}

fn file_and_module_graph() -> CodeGraph {
    CodeGraph {
        schema_version: 1,
        project: project(),
        nodes: vec![
            node(
                "file:src/main.rs",
                NodeKind::File,
                "src/main.rs",
                "src/main.rs",
                "src/main.rs",
                1,
            ),
            node(
                "module:codegraph",
                NodeKind::Module,
                "codegraph",
                "codegraph",
                "src/main.rs",
                1,
            ),
            node(
                "module:crabmap::model",
                NodeKind::Module,
                "model",
                "crabmap::model",
                "src/main.rs",
                12,
            ),
            node(
                "module:crabmap::model#2",
                NodeKind::Module,
                "crabmap::model",
                "crabmap::model",
                "src/model.rs",
                1,
            ),
            node(
                "function:crabmap::main_fn",
                NodeKind::Function,
                "main_fn",
                "crabmap::main_fn",
                "src/main.rs",
                20,
            ),
            node(
                "struct:crabmap::App",
                NodeKind::Struct,
                "App",
                "crabmap::App",
                "src/main.rs",
                30,
            ),
            node(
                "struct:crabmap::model::Node",
                NodeKind::Struct,
                "Node",
                "crabmap::model::Node",
                "src/model.rs",
                5,
            ),
        ],
        edges: vec![
            Edge {
                from: "file:src/main.rs".to_string(),
                to: "module:codegraph".to_string(),
                kind: EdgeKind::ModuleFile,
                label: None,
                evidence: None,
                weight: 1,
                source: EdgeSource::Ast,
                certainty: EdgeCertainty::Definite,
                profiles: vec![],
            },
            Edge {
                from: "module:codegraph".to_string(),
                to: "module:crabmap::model".to_string(),
                kind: EdgeKind::Declares,
                label: None,
                evidence: None,
                weight: 1,
                source: EdgeSource::Ast,
                certainty: EdgeCertainty::Definite,
                profiles: vec![],
            },
            Edge {
                from: "module:codegraph".to_string(),
                to: "function:crabmap::main_fn".to_string(),
                kind: EdgeKind::Declares,
                label: None,
                evidence: None,
                weight: 1,
                source: EdgeSource::Ast,
                certainty: EdgeCertainty::Definite,
                profiles: vec![],
            },
            Edge {
                from: "module:codegraph".to_string(),
                to: "struct:crabmap::App".to_string(),
                kind: EdgeKind::Declares,
                label: None,
                evidence: None,
                weight: 1,
                source: EdgeSource::Ast,
                certainty: EdgeCertainty::Definite,
                profiles: vec![],
            },
            Edge {
                from: "module:crabmap::model#2".to_string(),
                to: "struct:crabmap::model::Node".to_string(),
                kind: EdgeKind::Declares,
                label: None,
                evidence: None,
                weight: 1,
                source: EdgeSource::Ast,
                certainty: EdgeCertainty::Definite,
                profiles: vec![],
            },
        ],
        warnings: vec![],
        semantic: None,
        mir: None,
        profiles: vec![profile()],
        generated_at_ms: 0,
    }
}

fn project() -> Project {
    Project {
        root: "/tmp/demo".to_string(),
        workspace_root: "/tmp/demo".to_string(),
        packages: vec![Package {
            name: "codegraph".to_string(),
            manifest_path: "/tmp/demo/Cargo.toml".to_string(),
            targets: vec![Target {
                name: "codegraph".to_string(),
                kind: vec!["bin".to_string()],
                src_path: "src/main.rs".to_string(),
            }],
        }],
    }
}

fn profile() -> BuildProfile {
    BuildProfile {
        name: "default".to_string(),
        features: vec![],
        all_features: false,
        no_default_features: false,
        target: None,
    }
}

fn node(
    id: &str,
    kind: NodeKind,
    name: &str,
    qualified_name: &str,
    file: &str,
    line: usize,
) -> Node {
    Node {
        id: id.to_string(),
        kind,
        name: name.to_string(),
        qualified_name: qualified_name.to_string(),
        file: Some(file.to_string()),
        range: Some(Range {
            start_line: line,
            end_line: line,
        }),
        visibility: Some("pub".to_string()),
        signature: Some(format!("fn {name}()")),
        docs: None,
        metrics: BTreeMap::new(),
    }
}
