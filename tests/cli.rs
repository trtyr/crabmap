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

    let symbol = run(["query", "symbol", "run_app", "--graph", graph.to_str().unwrap()]);
    assert_eq!(symbol["kind"], "symbol");
    assert_eq!(symbol["node"]["name"], "run_app");

    let enum_symbol = run(["query", "symbol", "ConfigMode", "--graph", graph.to_str().unwrap()]);
    assert_eq!(enum_symbol["kind"], "symbol");
    assert!(
        enum_symbol["outgoing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["node"]["kind"] == "enum_member" && item["node"]["name"] == "Fast")
    );

    let callees = run(["query", "callees", "run_app", "--graph", graph.to_str().unwrap()]);
    assert_eq!(callees["kind"], "callees");
    assert!(
        callees["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["node"]["name"] == "load_config")
    );

    let search = run(["query", "search", "config", "--graph", graph.to_str().unwrap()]);
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
    let output = project.join(".ferrimind/ferrimind.json.gz");
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
            .join("server/.ferrimind/ferrimind.json.gz")
            .exists()
    );
    assert!(temp.path().join("agent/.ferrimind/ferrimind.json.gz").exists());
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

    let file = run(["query", "file", "src/main.rs", "--graph", graph.to_str().unwrap()]);
    let declares = file["declares"].as_array().unwrap();
    assert!(
        !declares.is_empty(),
        "expected src/main.rs declares to be non-empty: {file:#?}"
    );

    let module = run([
        "query",
        "module",
        "ferrimind::model",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    let module_declares = module["declares"].as_array().unwrap();
    assert!(
        !module_declares.is_empty(),
        "expected ferrimind::model declares to be non-empty: {module:#?}"
    );

    let callees = run([
        "query",
        "callees",
        "ferrimind::store::load_many",
        "--graph",
        graph.to_str().unwrap(),
    ]);
    let callee_names = callees["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["node"]["qualified_name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert!(callee_names.iter().any(|name| name == "ferrimind::store::load"));
    assert!(callee_names.iter().any(|name| name == "ferrimind::store::merge"));
    assert!(
        !callee_names
            .iter()
            .any(|name| name == "ferrimind::repo_map::map" || name == "ferrimind::config::load"),
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
    let output = Command::new(env!("CARGO_BIN_EXE_ferrimind"))
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

fn copy_dir(from: impl AsRef<Path>, to: impl AsRef<Path>) {
    std::fs::create_dir_all(to.as_ref()).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.as_ref().join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            if entry.file_name() == "target" || entry.file_name() == ".ferrimind" {
                continue;
            }
            copy_dir(entry.path(), target);
            continue;
        }
        std::fs::copy(entry.path(), target).unwrap();
    }
}
