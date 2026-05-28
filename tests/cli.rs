use serde_json::Value;
use std::path::Path;
use std::process::Command;

#[test]
fn indexes_and_queries_rust_project() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let temp = tempfile::tempdir().unwrap();
    let graph = temp.path().join("graph.json");

    let index = run([
        "index",
        root.to_str().unwrap(),
        "--output",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(index["kind"], "index");
    assert!(index["stats"]["nodes"].as_u64().unwrap() > 5);
    assert!(
        index["stats"]["by_edge"]["possible_dispatch"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert!(index["stats"]["by_source"]["inferred"].as_u64().unwrap() > 0);

    let symbol = run([
        "query",
        "symbol",
        "run_app",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(symbol["kind"], "symbol");
    assert_eq!(symbol["node"]["name"], "run_app");

    let enum_symbol = run([
        "query",
        "symbol",
        "ConfigMode",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(enum_symbol["kind"], "symbol");
    assert!(
        enum_symbol["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["node"]["kind"] == "enum_member" && item["node"]["name"] == "Fast")
    );

    let callees = run([
        "query",
        "callees",
        "run_app",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(callees["kind"], "callees");
    assert!(
        callees["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["node"]["name"] == "load_config")
    );

    let search = run([
        "query",
        "search",
        "config",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(search["kind"], "search");
    assert!(!search["items"].as_array().unwrap().is_empty());

    let entries = run(["nav", "entries", "--graph", graph.to_str().unwrap()]);
    assert_eq!(entries["kind"], "entries");
    assert!(
        entries["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["node"]["name"] == "run_app")
    );

    let clusters = run(["nav", "clusters", "--graph", graph.to_str().unwrap()]);
    assert_eq!(clusters["kind"], "clusters");
    assert!(!clusters["items"].as_array().unwrap().is_empty());

    let quality = run(["nav", "quality", "--graph", graph.to_str().unwrap()]);
    assert_eq!(quality["kind"], "quality");
    assert!(quality["score"].as_u64().unwrap() > 0);

    let guide = run(["nav", "guide", "config", "--graph", graph.to_str().unwrap()]);
    assert_eq!(guide["kind"], "guide");
    assert!(!guide["read_order"].as_array().unwrap().is_empty());

    let health = run(["nav", "health", "--graph", graph.to_str().unwrap()]);
    assert_eq!(health["kind"], "health");
    assert!(health["score"].as_u64().unwrap() <= 100);

    let report_dir = temp.path().join("report");
    let report = run([
        "nav",
        "report",
        "--graph",
        graph.to_str().unwrap(),
        "--output",
        report_dir.to_str().unwrap(),
    ]);
    assert_eq!(report["kind"], "report");
    assert!(report_dir.join("GRAPH_REPORT.md").exists());
    assert!(report_dir.join("AGENT_GUIDE.md").exists());

    let export = run([
        "query",
        "export",
        "--format",
        "mermaid",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(export["kind"], "mermaid");
    assert!(export["content"].as_str().unwrap().contains("graph LR"));
}

#[test]
fn records_build_profile_flags() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let temp = tempfile::tempdir().unwrap();
    let graph = temp.path().join("profile.json");

    let index = run([
        "index",
        root.to_str().unwrap(),
        "--all-features",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--output",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(index["kind"], "index");
    let export = run(["query", "export", "--graph", graph.to_str().unwrap()]);
    assert_eq!(export["profiles"][0]["name"], "default");
    assert_eq!(export["profiles"][0]["all_features"], true);
    assert_eq!(export["profiles"][0]["target"], "x86_64-unknown-linux-gnu");
}

#[test]
fn index_defaults_to_project_local_graph_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let temp = tempfile::tempdir().unwrap();
    copy_dir(&root, temp.path().join("sample"));
    let project = temp.path().join("sample");

    let index = run(["index", project.to_str().unwrap()]);
    assert_eq!(index["kind"], "index");
    let output = project.join(".crabmap/crabmap.json.gz");
    assert_eq!(
        Path::new(index["output"].as_str().unwrap())
            .canonicalize()
            .unwrap(),
        output.canonicalize().unwrap()
    );
    assert!(output.exists());
}

#[test]
fn queries_can_load_multiple_graphs() {
    let temp = tempfile::tempdir().unwrap();
    let root_one = temp.path().join("one");
    let root_two = temp.path().join("two");
    copy_dir(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample"),
        &root_one,
    );
    copy_dir(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample"),
        &root_two,
    );
    let graph_one = temp.path().join("one.json");
    let graph_two = temp.path().join("two.json");
    run([
        "index",
        root_one.to_str().unwrap(),
        "--output",
        graph_one.to_str().unwrap(),
    ]);
    run([
        "index",
        root_two.to_str().unwrap(),
        "--output",
        graph_two.to_str().unwrap(),
    ]);

    let search = run([
        "query",
        "search",
        "run_app",
        "--graph",
        graph_one.to_str().unwrap(),
        "--graph",
        graph_two.to_str().unwrap(),
    ]);
    let hits = search["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|item| item["name"] == "run_app")
        .count();
    assert_eq!(hits, 2);
}

#[test]
fn index_all_discovers_cargo_projects_without_overwriting_graphs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let temp = tempfile::tempdir().unwrap();
    copy_dir(&root, temp.path().join("server"));
    copy_dir(&root, temp.path().join("agent"));
    copy_dir(&root, temp.path().join(".worktrees/ignored/server"));

    let indexed = run([
        "index",
        temp.path().to_str().unwrap(),
        "--all",
        "--max-depth",
        "3",
    ]);
    assert_eq!(indexed["kind"], "index_all");
    assert_eq!(indexed["indexed"].as_array().unwrap().len(), 2);
    assert!(
        temp.path()
            .join("server/.crabmap/crabmap.json.gz")
            .exists()
    );
    assert!(
        temp.path()
            .join("agent/.crabmap/crabmap.json.gz")
            .exists()
    );
}

#[test]
fn self_index_reports_file_module_and_store_callees_correctly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let graph = temp.path().join("self.json");

    let index = run([
        "index",
        root.to_str().unwrap(),
        "--output",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(index["kind"], "index");

    let file = run([
        "query",
        "file",
        "src/main.rs",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    let declares = file["declares"].as_array().unwrap();
    assert!(
        !declares.is_empty(),
        "expected src/main.rs declares to be non-empty: {file:#?}"
    );

    let module = run([
        "query",
        "module",
        "crabmap::model",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    let module_declares = module["declares"].as_array().unwrap();
    assert!(
        !module_declares.is_empty(),
        "expected crabmap::model declares to be non-empty: {module:#?}"
    );

    let callees = run([
        "query",
        "callees",
        "crabmap::store::load_many",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    let callee_names = callees["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["node"]["qualified_name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(
        callee_names
            .iter()
            .any(|name| name == "crabmap::store::load")
    );
    assert!(
        callee_names
            .iter()
            .any(|name| name == "crabmap::store::merge")
    );
    assert!(
        !callee_names
            .iter()
            .any(|name| name == "crabmap::repo_map::map" || name == "crabmap::config::load"),
        "unexpected callees for store::load_many: {callee_names:?}"
    );
}

#[test]
fn mir_mode_confirms_lowered_calls_when_rustc_supports_unpretty_mir() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let probe = Command::new("cargo")
        .env("RUSTC_BOOTSTRAP", "1")
        .args([
            "rustc",
            "--manifest-path",
            root.join("Cargo.toml").to_str().unwrap(),
            "--lib",
            "--",
            "-Z",
            "unpretty=mir",
        ])
        .output()
        .unwrap();
    if !probe.status.success() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let graph = temp.path().join("mir.json");
    let index = run([
        "index",
        root.to_str().unwrap(),
        "--mir",
        "--mir-limit",
        "20",
        "--output",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(index["kind"], "index");
    assert_eq!(index["stats"]["mir"]["enabled"], true);
    assert!(
        index["stats"]["by_source"]["mir"]
            .as_u64()
            .unwrap_or_default()
            > 0
            || index["stats"]["mir"]["confirmed_edges"].as_u64().unwrap() > 0
    );
}

#[test]
fn semantic_mode_confirms_call_edges_when_rust_analyzer_is_available() {
    if Command::new("rust-analyzer")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let temp = tempfile::tempdir().unwrap();
    let graph = temp.path().join("semantic.json");

    let index = run([
        "index",
        root.to_str().unwrap(),
        "--semantic-limit",
        "20",
        "--output",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(index["kind"], "index");
    assert_eq!(index["stats"]["semantic"]["enabled"], true);
    assert!(
        index["stats"]["semantic"]["confirmed_symbols"]
            .as_u64()
            .unwrap()
            > 0
    );
}

fn run<const N: usize>(args: [&str; N]) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_crabmap"))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Helper: index the fixture project, return (graph_path, temp_dir).
/// Temp dir must stay alive to keep the graph file accessible.
fn index_fixture() -> (std::path::PathBuf, tempfile::TempDir) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
    let temp = tempfile::tempdir().unwrap();
    let graph = temp.path().join("graph.json");
    run([
        "index",
        root.to_str().unwrap(),
        "--output",
        graph.to_str().unwrap(),
    ]);
    (graph, temp)
}

fn copy_dir(from: impl AsRef<Path>, to: impl AsRef<Path>) {
    std::fs::create_dir_all(to.as_ref()).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.as_ref().join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() == "target" || entry.file_name() == ".crabmap" {
                continue;
            }
            copy_dir(entry.path(), target);
            continue;
        }
        std::fs::copy(entry.path(), target).unwrap();
    }
}

// ---------------------------------------------------------------------------
// 1:1 integration tests — one test per untested CLI command
// ---------------------------------------------------------------------------

// ---- query ----------------------------------------------------------------

#[test]
fn query_summary_returns_hot_symbols_and_project_info() {
    let (graph, _temp) = index_fixture();
    let out = run(["query", "summary", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "summary");
    assert!(out["hot_symbols"].is_array());
    assert!(out["project"].is_object());
    assert!(out["stats"].is_object());
    assert!(out["top_files"].is_array());
}

#[test]
fn query_symbols_lists_all_and_filters_by_kind() {
    let (graph, _temp) = index_fixture();

    let all = run(["query", "symbols", "--graph", graph.to_str().unwrap()]);
    assert_eq!(all["kind"], "symbols");
    let items = all["items"].as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(item["id"].is_string(), "missing id: {item:#?}");
        assert!(item["name"].is_string(), "missing name: {item:#?}");
        assert!(item["kind"].is_string(), "missing kind: {item:#?}");
    }

    let fns = run([
        "query",
        "symbols",
        "--kind",
        "function",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(fns["kind"], "symbols");
    for item in fns["items"].as_array().unwrap() {
        assert_eq!(item["kind"], "function");
    }
}

#[test]
fn query_callers_finds_upstream_callers() {
    let (graph, _temp) = index_fixture();
    let out = run([
        "query",
        "callers",
        "load_config",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(out["kind"], "callers");
    let items = out["items"].as_array().unwrap();
    assert!(
        items.iter().any(|item| item["node"]["name"] == "run_app"),
        "expected run_app as caller of load_config: {items:#?}"
    );
}

#[test]
fn query_impact_returns_dependency_analysis() {
    let (graph, _temp) = index_fixture();
    let out = run([
        "query",
        "impact",
        "load_config",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(out["kind"], "impact");
    assert!(out["root"].is_object());
    assert!(out["dependencies"].is_array());
    assert!(out["dependents"].is_array());
    assert!(out["callers"].is_array());
}

#[test]
fn query_path_finds_shortest_path_between_symbols() {
    let (graph, _temp) = index_fixture();
    let out = run([
        "query",
        "path",
        "run_app",
        "load_config",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(out["kind"], "path");
    assert_eq!(out["found"], true);
    assert!(out["nodes"].is_array());
    assert!(!out["nodes"].as_array().unwrap().is_empty());
    assert!(out["from"].is_object());
    assert!(out["to"].is_object());
}

#[test]
fn query_export_dot_produces_digraph() {
    let (graph, _temp) = index_fixture();
    let out = run([
        "query",
        "export",
        "--format",
        "dot",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(out["kind"], "dot");
    assert!(
        out["content"].as_str().unwrap().starts_with("digraph"),
        "expected dot output to start with 'digraph'"
    );
}

#[test]
fn query_export_json_produces_full_graph_data() {
    let (graph, _temp) = index_fixture();
    let out = run([
        "query",
        "export",
        "--format",
        "json",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    // JSON export is raw graph data — no "kind" field
    assert!(out["nodes"].is_array());
    assert!(out["edges"].is_array());
    assert!(out["project"].is_object());
    assert!(
        out["schema_version"].is_string() || out["schema_version"].is_number(),
        "expected schema_version: {out:#?}"
    );
}

// ---- nav ------------------------------------------------------------------

#[test]
fn nav_map_produces_token_budgeted_content() {
    let (graph, _temp) = index_fixture();
    let out = run(["nav", "map", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "map");
    assert!(!out["content"].as_str().unwrap().is_empty());
    assert_eq!(out["budget"], 8000);
}

// ---- analyze --------------------------------------------------------------

#[test]
fn analyze_deps_shows_module_dependency_matrix() {
    let (graph, _temp) = index_fixture();
    let out = run(["analyze", "deps", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "deps");
    let items = out["items"].as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(item["from"].is_string());
        assert!(item["to"].is_string());
        assert!(item["weight"].is_number());
    }
}

#[test]
fn analyze_fanout_shows_file_fan_metrics() {
    let (graph, _temp) = index_fixture();
    let out = run(["analyze", "fanout", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "fanout");
    let items = out["items"].as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(item["file"].is_string());
        assert!(item["fanin"].is_number());
        assert!(item["fanout"].is_number());
        assert!(item["total"].is_number());
    }
}

#[test]
fn analyze_tests_finds_test_candidates() {
    let (graph, _temp) = index_fixture();
    let out = run(["analyze", "tests", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "tests");
    assert!(out["candidate_tests"].is_array());
    assert!(out["targets"].is_array());
}

#[test]
fn analyze_hotspots_shows_git_churn() {
    let (graph, _temp) = index_fixture();
    let out = run(["analyze", "hotspots", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "git");
    assert!(out["hotspots"].is_array());
    assert!(out["cochange"].is_array());
}

#[test]
fn analyze_diff_shows_graph_diff_against_git() {
    let (graph, _temp) = index_fixture();
    let out = run(["analyze", "diff", "--graph", graph.to_str().unwrap()]);
    assert_eq!(out["kind"], "diff");
    assert!(out["added_edges"].is_array());
    assert!(out["removed_edges"].is_array());
    assert!(out["changed_files"].is_array());
}

// ---- config ---------------------------------------------------------------

#[test]
fn config_show_returns_current_config() {
    let out = run(["config"]);
    assert_eq!(out["kind"], "config");
    assert!(out["config"].is_object());
    assert!(out["path"].is_string());
}

// ---- error handling -------------------------------------------------------

#[test]
fn error_symbol_not_found_suggests_alternatives() {
    let (graph, _temp) = index_fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_crabmap"))
        .args([
            "query",
            "symbol",
            "nonexistent_xyz",
            "--graph",
            graph.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "expected non-zero exit for missing symbol"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr: {stderr}");
    assert!(stderr.contains("Did you mean?"), "stderr: {stderr}");
}

#[test]
fn error_ambiguous_symbol_lists_matches() {
    let (graph, _temp) = index_fixture();
    // "save" is declared as both Store::save (trait) and MemoryStore::save (impl)
    // The tool returns exit code 0 with kind="ambiguous" in JSON
    let out = run([
        "query",
        "symbol",
        "save",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    assert_eq!(out["kind"], "ambiguous");
    let matches = out["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    let names: Vec<&str> = matches
        .iter()
        .map(|m| m["qualified_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"sample::Store::save"));
    assert!(names.contains(&"sample::MemoryStore::save"));
}
