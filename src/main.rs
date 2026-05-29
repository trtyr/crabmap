mod ai;
mod analyzer;
mod cli;
mod config;
mod deps;
mod drift;
mod export;
mod gitintel;
mod health;
mod llm;
mod mir;
mod model;
mod query;
mod rag;
mod repo_map;
mod report;
mod semantic;
mod store;
mod term;
mod test_impact;
mod web;

use anyhow::Result;
use clap::Parser;
use cli::{AnalyzeCmd, Cli, Command, NavCmd, QueryCmd};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn main() {
    if let Err(error) = run() {
        if term::use_colors() {
            eprintln!("{} {error:#}", term::red("error:"));
        } else {
            eprintln!("error: {error:#}");
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let result = (|| match cli.command {
        Command::Index(args) => {
            if args.index.all {
                let projects = find_cargo_projects(&args.project, args.index.max_depth)?;
                let output_dir = args.index.output_dir.clone();
                let mut indexed = Vec::new();
                let mut failures = Vec::new();
                for project in projects {
                    let output = output_dir
                        .as_ref()
                        .map(|dir| dir.join(format!("{}.json", graph_filename(&project))));
                    match index_project(IndexRequest {
                        project: project.clone(),
                        output,
                        include_tests: !args.index.no_tests,
                        features: args.index.features.clone(),
                        all_features: args.index.all_features,
                        no_default_features: args.index.no_default_features,
                        target: args.index.target.clone(),
                        profile: "default".to_string(),
                        semantic: !args.index.no_semantic,
                        semantic_limit: args.index.semantic_limit,
                        mir: args.index.mir,
                        mir_limit: args.index.mir_limit,
                    }) {
                        Ok((graph, output)) => indexed.push(json!({
                            "project": project,
                            "output": output,
                            "stats": graph.stats(),
                            "warnings": graph.warnings,
                        })),
                        Err(error) => failures.push(json!({
                            "project": project,
                            "error": format!("{error:#}"),
                        })),
                    }
                }
                Ok(json!({
                    "kind": "index_all",
                    "root": args.project,
                    "indexed": indexed,
                    "failures": failures,
                }))
            } else {
                let (graph, output) = index_project(IndexRequest {
                    project: args.project,
                    output: args.output,
                    include_tests: !args.index.no_tests,
                    features: args.index.features,
                    all_features: args.index.all_features,
                    no_default_features: args.index.no_default_features,
                    target: args.index.target,
                    profile: "default".to_string(),
                    semantic: !args.index.no_semantic,
                    semantic_limit: args.index.semantic_limit,
                    mir: args.index.mir,
                    mir_limit: args.index.mir_limit,
                })?;
                Ok(json!({
                    "kind": "index",
                    "project": graph.project.root,
                    "output": output,
                    "stats": graph.stats(),
                    "warnings": graph.warnings
                }))
            }
        }
        Command::Serve(args) => {
            web::serve(args)?;
            Ok(json!({ "kind": "serve", "stopped": true }))
        }
        Command::Query(cmd) => match cmd {
            QueryCmd::Stats(args) => Ok(json!({
                "kind": "stats",
                "stats": store::load_many(&args.graph)?.stats()
            })),
            QueryCmd::Summary(args) => {
                Ok(query::summary(&store::load_many(&args.graph)?, args.limit))
            }
            QueryCmd::Symbols(args) => Ok(query::symbols(
                &store::load_many(&args.graph)?,
                args.query.as_deref(),
                args.kind.as_deref(),
                args.limit,
                crate::query::SymbolFilter {
                    visibility: args.visibility,
                    no_docs: args.no_docs,
                    dead: args.dead,
                    test_only: args.test_only,
                    min_callers: args.min_callers,
                    max_callers: args.max_callers,
                    min_degree: args.min_degree,
                    max_degree: args.max_degree,
                },
            )),
            QueryCmd::Inspect(args) => {
                Ok(query::inspect(&store::load_many(&args.graph)?, &args.name, !args.no_source)?)
            }
            QueryCmd::Trace(args) => {
                Ok(query::trace(&store::load_many(&args.graph)?, &args.name, args.direction, args.depth, args.limit)?)
            }
            QueryCmd::Find(args) => {
                Ok(query::find(&store::load_many(&args.graph)?, &args.pattern, args.mode, args.limit))
            }
            QueryCmd::Scope(args) => {
                Ok(query::scope(&store::load_many(&args.graph)?, &args.target, args.kind)?)
            }
            QueryCmd::Impact(args) => Ok(query::impact(
                &store::load_many(&args.graph)?,
                &args.name,
                args.depth,
                args.limit,
            )?),
            QueryCmd::Risk(args) => Ok(query::risk(
                &store::load_many(&args.graph)?,
                &args.name,
                args.depth,
                args.limit,
            )?),
            QueryCmd::Path(args) => Ok(query::path(
                &store::load_many(&args.graph)?,
                &args.from,
                &args.to,
                args.depth,
            )?),
            QueryCmd::Export(args) => export::export(&store::load_many(&args.graph)?, args.format),
        },
        Command::Nav(cmd) => match cmd {
            NavCmd::Guide(args) => {
                let graph = store::load_many(&args.graph)?;
                if let Some(query) = args.query.as_deref() {
                    if let Ok(roots) = rag::retrieve_roots(
                        &graph,
                        &config::load()?,
                        query,
                        args.limit.min(8),
                        args.limit * 6,
                    ) {
                        let root_ids = roots.into_iter().map(|root| root.id).collect::<Vec<_>>();
                        return Ok(ai::guide_with_root_ids(
                            &graph,
                            Some(query),
                            &root_ids,
                            args.depth,
                            args.limit,
                            "rag",
                        ));
                    }
                }
                Ok(ai::guide(
                    &graph,
                    args.query.as_deref(),
                    args.depth,
                    args.limit,
                ))
            }
            NavCmd::Quality(args) => Ok(ai::quality(&store::load_many(&args.graph)?)),
            NavCmd::Health(args) => Ok(health::health(&store::load_many(&args.graph)?, args.limit)),
            NavCmd::Report(args) => {
                report::write(&store::load_many(&args.graph)?, args.output.as_deref())
            }
            NavCmd::Map(args) => repo_map::map(&store::load_many(&args.graph)?, args.budget, args.full),
            NavCmd::Ask(args) => Ok(llm::ask(
                &store::load_many(&args.graph)?,
                &config::load()?,
                &args.question,
                args.depth,
                args.limit,
                args.max_tokens,
            )?),
            NavCmd::Retrieve(args) => Ok(rag::retrieve(
                &store::load_many(&args.graph)?,
                &config::load()?,
                &args.query,
                args.limit,
                args.candidates,
            )?),
        },
        Command::Analyze(cmd) => match cmd {
            AnalyzeCmd::Deps(args) => Ok(deps::deps(
                &store::load_many(&args.graph)?,
                args.from.as_deref(),
                args.to.as_deref(),
                args.limit,
            )),
            AnalyzeCmd::Fanout(args) => {
                Ok(deps::fanout(&store::load_many(&args.graph)?, args.limit))
            }
            AnalyzeCmd::Tests(args) => Ok(test_impact::tests(
                &store::load_many(&args.graph)?,
                args.symbol.as_deref(),
                args.limit,
            )),
            AnalyzeCmd::Hotspots(args) => gitintel::git_intel(
                &store::load_many(&args.graph)?,
                args.project.as_deref(),
                args.limit,
            ),
            AnalyzeCmd::Diff(args) => drift::diff(
                &store::load_many(&args.graph)?,
                args.project.as_deref(),
                &args.base,
                args.limit,
            ),
            AnalyzeCmd::RefactorOrder(args) => Ok(query::refactor_order(
                &store::load_many(&args.graph)?,
                &args.symbols,
                args.limit,
            )?),
        },
        Command::Config(args) => {
            if args.api_key.is_some()
                || args.model.is_some()
                || args.api_url.is_some()
                || args.embedding_key.is_some()
                || args.embedding_model.is_some()
                || args.embedding_url.is_some()
                || args.rerank_key.is_some()
                || args.rerank_model.is_some()
                || args.rerank_url.is_some()
            {
                config::update(
                    args.api_key,
                    args.model,
                    args.api_url,
                    args.embedding_key,
                    args.embedding_model,
                    args.embedding_url,
                    args.rerank_key,
                    args.rerank_model,
                    args.rerank_url,
                )
            } else {
                config::show()
            }
        }
    })();

    println!("{}", serde_json::to_string_pretty(&result?)?);
    Ok(())
}

struct IndexRequest {
    project: PathBuf,
    output: Option<PathBuf>,
    include_tests: bool,
    features: Vec<String>,
    all_features: bool,
    no_default_features: bool,
    target: Option<String>,
    profile: String,
    semantic: bool,
    semantic_limit: usize,
    mir: bool,
    mir_limit: usize,
}

fn index_project(input: IndexRequest) -> Result<(model::CodeGraph, PathBuf)> {
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:30.cyan/blue}] {pos}/{len} files ({eta})")
            .unwrap()
            .progress_chars("━●")
    );
    let pb2 = pb.clone();
    let mut graph = analyzer::index_project(
        &input.project,
        analyzer::IndexOptions {
            include_tests: input.include_tests,
            features: input.features,
            all_features: input.all_features,
            no_default_features: input.no_default_features,
            target: input.target,
            profile: input.profile,
            on_progress: Some(Arc::new(move |current: usize, total: usize| {
                pb2.set_length(total as u64);
                pb2.set_position(current as u64);
            })),
        },
    )?;
    pb.finish_and_clear();
    let warn_msg = if graph.warnings.is_empty() {
        String::new()
    } else if term::use_colors() {
        format!(
            ", {}",
            term::yellow(&format!("{} warnings", graph.warnings.len()))
        )
    } else {
        format!(", {} warnings", graph.warnings.len())
    };
    if term::use_colors() {
        eprintln!(
            "{} indexed {} nodes, {} edges in {} files{}",
            term::green("✓"),
            term::bold(&graph.nodes.len().to_string()),
            term::bold(&graph.edges.len().to_string()),
            term::bold(&graph.stats().files.to_string()),
            warn_msg
        );
    } else {
        eprintln!(
            "indexed {} nodes, {} edges in {} files{}",
            graph.nodes.len(),
            graph.edges.len(),
            graph.stats().files,
            warn_msg
        );
    }
    if input.semantic {
        semantic::enrich(&mut graph, &input.project, input.semantic_limit)?;
    }
    if input.mir {
        mir::enrich(&mut graph, &input.project, input.mir_limit)?;
    }
    let output = store::save_project(&input.project, input.output.as_deref(), &graph)?;
    Ok((graph, output))
}

fn find_cargo_projects(root: &Path, max_depth: usize) -> Result<Vec<PathBuf>> {
    let root = root.canonicalize()?;
    let mut projects = Vec::new();
    collect_cargo_projects(&root, &root, max_depth, &mut projects)?;
    projects.sort();
    projects.dedup();
    Ok(projects)
}

fn collect_cargo_projects(
    root: &Path,
    dir: &Path,
    max_depth: usize,
    projects: &mut Vec<PathBuf>,
) -> Result<()> {
    let depth = dir.strip_prefix(root).unwrap_or(dir).components().count();
    if depth > max_depth {
        return Ok(());
    }
    if dir.join("Cargo.toml").exists() {
        projects.push(dir.to_path_buf());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if matches!(
            name.to_string_lossy().as_ref(),
            ".git" | ".crabmap" | ".worktrees" | "target" | "node_modules"
        ) {
            continue;
        }
        collect_cargo_projects(root, &entry.path(), max_depth, projects)?;
    }
    Ok(())
}

fn graph_filename(project: &Path) -> String {
    project
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("crabmap")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
