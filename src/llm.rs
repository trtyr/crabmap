use crate::ai;
use crate::config::CodegraphConfig;
use crate::model::{CodeGraph, EdgeKind, Node};
use crate::rag;
use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Serialize)]
struct MessageRequest {
    model: String,
    max_tokens: usize,
    messages: Vec<Message>,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct MessageResponse {
    content: Vec<ContentBlock>,
    #[serde(default)]
    usage: Option<Value>,
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

pub fn ask(
    graph: &CodeGraph,
    config: &CodegraphConfig,
    question: &str,
    depth: usize,
    limit: usize,
    max_tokens: usize,
) -> Result<Value> {
    let api_key = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
        .context("codegraph config is missing api_key; run `codegraph config --api-key ...`")?;
    let guide = ai::guide(graph, Some(question), depth, limit);
    let clusters = ai::clusters(graph, 8);
    let quality = ai::quality(graph);
    let retrieval = rag::retrieve(graph, config, question, limit.min(12), (limit * 6).max(40))
        .unwrap_or_else(|error| json!({ "error": format!("{error:#}") }));
    let compact_guide = compact_guide(&guide, limit);
    let compact_clusters = compact_clusters(&clusters);
    let compact_quality = compact_quality(&quality);
    let compact_retrieval = compact_retrieval(&retrieval, limit.min(12));
    let symbol_facts = symbol_facts(graph, question, limit.min(16));
    let prompt = prompt(
        question,
        &compact_guide,
        &compact_clusters,
        &compact_quality,
        &compact_retrieval,
        &symbol_facts,
    )?;
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("failed to build HTTP client")?;
    let response = client
        .post(&config.api_url)
        .bearer_auth(api_key)
        .json(&MessageRequest {
            model: config.model.clone(),
            max_tokens,
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
        })
        .send()
        .with_context(|| format!("failed to call {}", config.api_url))?;
    let status = response.status();
    let body = response.text().context("failed to read LLM response")?;
    if !status.is_success() {
        bail!("LLM request failed with {status}: {body}");
    }
    let data = serde_json::from_str::<MessageResponse>(&body)
        .with_context(|| format!("failed to parse LLM response: {body}"))?;
    let answer = data
        .content
        .into_iter()
        .filter(|block| block.kind == "text")
        .filter_map(|block| block.text)
        .collect::<Vec<_>>()
        .join("\n");
    if answer.trim().is_empty() {
        bail!(
            "LLM returned no text content; increase --max-tokens or use a non-thinking model. stop_reason={:?}",
            data.stop_reason
        );
    }
    Ok(json!({
        "kind": "ask",
        "model": config.model,
        "answer": answer,
        "context": {
            "guide": compact_guide,
            "clusters": compact_clusters,
            "quality": compact_quality,
            "retrieval": compact_retrieval,
            "symbol_facts": symbol_facts
        },
        "usage": data.usage,
        "stop_reason": data.stop_reason
    }))
}

fn prompt(
    question: &str,
    guide: &Value,
    clusters: &Value,
    quality: &Value,
    retrieval: &Value,
    symbol_facts: &Value,
) -> Result<String> {
    Ok(format!(
        r#"你是一个 Rust 项目代码图谱导航助手。

目标：
- 根据 codegraph 的结构化上下文回答用户问题。
- 优先给出“应该读哪些文件/符号、按什么顺序读、为什么”。
- 如果图谱已经给出签名、枚举成员、结构字段、trait/impl 方法或文档，直接列出这些具体定义。
- 不要把“回源码确认”当作默认回答；只有图谱没有给出足够事实时才提示继续读源码。
- 输出中文。
- 不要使用 emoji。
- 保持简洁，先回答具体事实，再列出必要的阅读顺序和原因。

用户问题：
{question}

图谱质量：
{quality}

功能簇：
{clusters}

RAG 检索结果：
{retrieval}

符号事实：
{symbol_facts}

导航上下文：
{guide}
"#,
        quality = serde_json::to_string_pretty(quality)?,
        clusters = serde_json::to_string_pretty(clusters)?,
        retrieval = serde_json::to_string_pretty(retrieval)?,
        symbol_facts = serde_json::to_string_pretty(symbol_facts)?,
        guide = serde_json::to_string_pretty(guide)?,
    ))
}

fn symbol_facts(graph: &CodeGraph, question: &str, limit: usize) -> Value {
    let terms = question
        .to_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|term| term.len() > 2)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut matched = graph
        .nodes
        .iter()
        .filter(|node| {
            let haystack = format!(
                "{} {} {} {}",
                node.name,
                node.qualified_name,
                node.file.as_deref().unwrap_or_default(),
                node.docs.as_deref().unwrap_or_default()
            )
            .to_lowercase();
            terms.iter().any(|term| haystack.contains(term))
        })
        .take(limit)
        .collect::<Vec<_>>();
    if matched.is_empty() {
        matched = graph
            .nodes
            .iter()
            .filter(|node| node.signature.is_some())
            .take(limit)
            .collect();
    }
    let items = matched
        .into_iter()
        .map(|node| {
            let children = graph
                .edges
                .iter()
                .filter(|edge| {
                    edge.from == node.id
                        && matches!(
                            edge.kind,
                            EdgeKind::Declares | EdgeKind::HasMethod | EdgeKind::Contains
                        )
                })
                .filter_map(|edge| graph.nodes.iter().find(|candidate| candidate.id == edge.to))
                .take(40)
                .map(symbol_fact_node)
                .collect::<Vec<_>>();
            json!({
                "node": symbol_fact_node(node),
                "children": children,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "items": items,
        "note": "These are concrete graph facts. Use them directly when they answer the question."
    })
}

fn symbol_fact_node(node: &Node) -> Value {
    json!({
        "kind": node.kind.as_str(),
        "name": node.name,
        "qualified_name": node.qualified_name,
        "file": node.file,
        "range": node.range,
        "signature": node.signature,
        "docs": node.docs,
    })
}

fn compact_quality(value: &Value) -> Value {
    json!({
        "score": value.get("score"),
        "interpretation": value.get("interpretation"),
        "trusted_edges": value.get("trusted_edges"),
        "uncertain_edges": value.get("uncertain_edges"),
        "recommendations": value.get("recommendations")
    })
}

fn compact_clusters(value: &Value) -> Value {
    json!({
        "items": value
            .get("items")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
            .iter()
            .take(8)
            .map(|item| {
                json!({
                    "name": item.get("name"),
                    "files": item.get("files"),
                    "symbols": item.get("symbols"),
                    "degree": item.get("degree"),
                    "hot_symbols": item
                        .get("hot_symbols")
                        .and_then(Value::as_array)
                        .unwrap_or(&Vec::new())
                        .iter()
                        .take(5)
                        .map(compact_node)
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
    })
}

fn compact_guide(value: &Value, limit: usize) -> Value {
    json!({
        "query": value.get("query"),
        "roots": compact_nodes(value.get("roots"), limit.min(8)),
        "read_order": value
            .get("read_order")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
            .iter()
            .take(limit)
            .map(|item| {
                json!({
                    "reason": item.get("reason"),
                    "file": item.get("file"),
                    "range": item.get("range"),
                    "node": compact_node(item.get("node").unwrap_or(&Value::Null))
                })
            })
            .collect::<Vec<_>>(),
        "callers": compact_walk(value.get("callers"), limit),
        "callees": compact_walk(value.get("callees"), limit),
        "impact": compact_walk(value.get("impact"), limit)
    })
}

fn compact_retrieval(value: &Value, limit: usize) -> Value {
    json!({
        "stages": value.get("stages"),
        "error": value.get("error"),
        "items": value
            .get("items")
            .and_then(Value::as_array)
            .unwrap_or(&Vec::new())
            .iter()
            .take(limit)
            .map(|item| {
                json!({
                    "score": item.get("score"),
                    "node": compact_node(item.get("node").unwrap_or(&Value::Null))
                })
            })
            .collect::<Vec<_>>()
    })
}

fn compact_nodes(value: Option<&Value>, limit: usize) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(limit)
        .map(compact_node)
        .collect()
}

fn compact_walk(value: Option<&Value>, limit: usize) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .take(limit)
        .map(|item| {
            json!({
                "depth": item.get("depth"),
                "edge": item.get("edge").map(compact_edge),
                "node": compact_node(item.get("node").unwrap_or(&Value::Null))
            })
        })
        .collect()
}

fn compact_node(value: &Value) -> Value {
    json!({
        "kind": value.get("kind"),
        "name": value.get("name"),
        "qualified_name": value.get("qualified_name"),
        "file": value.get("file"),
        "range": value.get("range"),
        "degree": value.get("degree")
    })
}

fn compact_edge(value: &Value) -> Value {
    json!({
        "kind": value.get("kind"),
        "source": value.get("source"),
        "certainty": value.get("certainty"),
        "evidence": value.get("evidence")
    })
}
