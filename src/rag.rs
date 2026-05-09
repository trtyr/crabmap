use crate::config::{CodegraphConfig, ModelProvider};
use crate::model::{CodeGraph, Node, NodeKind};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::cmp::Ordering;
use std::time::Duration;

#[derive(Clone)]
struct Document<'a> {
    node: &'a Node,
    text: String,
    lexical: f64,
    embedding: Option<f64>,
    rerank: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct RetrievedRoot {
    pub id: String,
}

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
    #[serde(default)]
    usage: Option<Value>,
}

#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f64>,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Serialize)]
struct RerankRequest {
    model: String,
    query: String,
    documents: Vec<String>,
    top_n: usize,
    return_documents: bool,
}

#[derive(Deserialize)]
struct RerankResponse {
    #[serde(default)]
    results: Vec<RerankItem>,
    #[serde(default)]
    data: Vec<RerankItem>,
    #[serde(default)]
    usage: Option<Value>,
}

#[derive(Deserialize)]
struct RerankItem {
    index: usize,
    #[serde(default)]
    relevance_score: Option<f64>,
    #[serde(default)]
    score: Option<f64>,
}

pub fn retrieve(
    graph: &CodeGraph,
    config: &CodegraphConfig,
    query: &str,
    limit: usize,
    candidates: usize,
) -> Result<Value> {
    let mut docs = lexical_candidates(graph, query, candidates.max(limit));
    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .context("failed to build HTTP client")?;
    let mut stages = vec!["lexical".to_string()];
    let mut usage = Vec::new();
    if let Some(provider) = configured_provider(config.embedding.as_ref()) {
        if let Some(embedding_usage) = apply_embedding(&client, provider, query, &mut docs)? {
            usage.push(json!({ "stage": "embedding", "usage": embedding_usage }));
        }
        stages.push("embedding".to_string());
        sort_docs(&mut docs);
        docs.truncate(candidates.min(48).max(limit));
    }
    if let Some(provider) = configured_provider(config.rerank.as_ref()) {
        if let Some(rerank_usage) = apply_rerank(&client, provider, query, &mut docs, limit)? {
            usage.push(json!({ "stage": "rerank", "usage": rerank_usage }));
        }
        stages.push("rerank".to_string());
    }
    sort_docs(&mut docs);
    Ok(json!({
        "kind": "retrieve",
        "query": query,
        "stages": stages,
        "items": docs.into_iter().take(limit).map(document_value).collect::<Vec<_>>(),
        "usage": usage
    }))
}

pub fn retrieve_roots(
    graph: &CodeGraph,
    config: &CodegraphConfig,
    query: &str,
    limit: usize,
    candidates: usize,
) -> Result<Vec<RetrievedRoot>> {
    let value = retrieve(graph, config, query, limit, candidates)?;
    Ok(value
        .get("items")
        .and_then(Value::as_array)
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|item| {
            item.get("node")
                .and_then(|node| node.get("id"))
                .and_then(Value::as_str)
                .map(|id| RetrievedRoot { id: id.to_string() })
        })
        .collect())
}

fn configured_provider(provider: Option<&ModelProvider>) -> Option<&ModelProvider> {
    provider.and_then(|provider| {
        provider
            .api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
            .then_some(provider)
    })
}

fn lexical_candidates<'a>(graph: &'a CodeGraph, query: &str, limit: usize) -> Vec<Document<'a>> {
    let terms = terms(query);
    let mut docs = graph
        .nodes
        .iter()
        .filter(|node| is_retrievable(node))
        .filter_map(|node| {
            let text = document_text(node);
            let score = lexical_score(&text, &terms) + structural_score(node);
            (score > 0.0).then_some(Document {
                node,
                text,
                lexical: score,
                embedding: None,
                rerank: None,
            })
        })
        .collect::<Vec<_>>();
    sort_docs(&mut docs);
    docs.truncate(limit);
    docs
}

fn apply_embedding(
    client: &Client,
    provider: &ModelProvider,
    query: &str,
    docs: &mut [Document],
) -> Result<Option<Value>> {
    if docs.is_empty() {
        return Ok(None);
    }
    let mut input = vec![query.to_string()];
    input.extend(docs.iter().map(|doc| doc.text.clone()));
    let response = client
        .post(&provider.api_url)
        .bearer_auth(provider.api_key.as_deref().unwrap_or_default())
        .json(&EmbeddingRequest {
            model: provider.model.clone(),
            input,
        })
        .send()
        .with_context(|| format!("failed to call embedding endpoint {}", provider.api_url))?;
    let status = response.status();
    let body = response
        .text()
        .context("failed to read embedding response")?;
    if !status.is_success() {
        return Ok(Some(
            json!({ "error": format!("embedding request failed with {status}: {body}") }),
        ));
    }
    let data = serde_json::from_str::<EmbeddingResponse>(&body)
        .with_context(|| format!("failed to parse embedding response: {body}"))?;
    let query_vector = data
        .data
        .iter()
        .find(|item| item.index == Some(0))
        .or_else(|| data.data.first())
        .map(|item| item.embedding.as_slice())
        .unwrap_or(&[]);
    if query_vector.is_empty() {
        return Ok(data.usage);
    }
    for (offset, doc) in docs.iter_mut().enumerate() {
        let target_index = offset + 1;
        let vector = data
            .data
            .iter()
            .find(|item| item.index == Some(target_index))
            .or_else(|| data.data.get(target_index))
            .map(|item| item.embedding.as_slice())
            .unwrap_or(&[]);
        if !vector.is_empty() {
            doc.embedding = Some(cosine(query_vector, vector));
        }
    }
    Ok(data.usage)
}

fn apply_rerank(
    client: &Client,
    provider: &ModelProvider,
    query: &str,
    docs: &mut [Document],
    limit: usize,
) -> Result<Option<Value>> {
    if docs.is_empty() {
        return Ok(None);
    }
    let response = client
        .post(&provider.api_url)
        .bearer_auth(provider.api_key.as_deref().unwrap_or_default())
        .json(&RerankRequest {
            model: provider.model.clone(),
            query: query.to_string(),
            documents: docs.iter().map(|doc| doc.text.clone()).collect(),
            top_n: limit.max(1),
            return_documents: false,
        })
        .send()
        .with_context(|| format!("failed to call rerank endpoint {}", provider.api_url))?;
    let status = response.status();
    let body = response.text().context("failed to read rerank response")?;
    if !status.is_success() {
        return Ok(Some(
            json!({ "error": format!("rerank request failed with {status}: {body}") }),
        ));
    }
    let data = serde_json::from_str::<RerankResponse>(&body)
        .with_context(|| format!("failed to parse rerank response: {body}"))?;
    let results = if data.results.is_empty() {
        data.data
    } else {
        data.results
    };
    for item in results {
        if let Some(doc) = docs.get_mut(item.index) {
            doc.rerank = item.relevance_score.or(item.score);
        }
    }
    Ok(data.usage)
}

fn sort_docs(docs: &mut [Document]) {
    docs.sort_by(|a, b| {
        score(b)
            .partial_cmp(&score(a))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.node.qualified_name.cmp(&b.node.qualified_name))
    });
}

fn score(doc: &Document) -> f64 {
    doc.rerank
        .map(|score| score * 1000.0)
        .or_else(|| doc.embedding.map(|score| score * 100.0 + doc.lexical))
        .unwrap_or(doc.lexical)
}

fn document_value(doc: Document) -> Value {
    json!({
        "score": score(&doc),
        "lexical_score": doc.lexical,
        "embedding_score": doc.embedding,
        "rerank_score": doc.rerank,
        "node": {
            "id": doc.node.id,
            "kind": doc.node.kind.as_str(),
            "name": doc.node.name,
            "qualified_name": doc.node.qualified_name,
            "file": doc.node.file,
            "range": doc.node.range,
            "visibility": doc.node.visibility,
            "degree": doc.node.metrics.get("degree")
        },
        "text": doc.text
    })
}

fn document_text(node: &Node) -> String {
    [
        format!("kind: {}", node.kind.as_str()),
        format!("name: {}", node.name),
        format!("qualified_name: {}", node.qualified_name),
        format!("file: {}", node.file.as_deref().unwrap_or_default()),
        format!(
            "range: {}",
            node.range
                .as_ref()
                .map(|range| format!("{}-{}", range.start_line, range.end_line))
                .unwrap_or_default()
        ),
        format!(
            "signature: {}",
            truncate(node.signature.as_deref().unwrap_or_default(), 420)
        ),
        format!(
            "docs: {}",
            truncate(node.docs.as_deref().unwrap_or_default(), 420)
        ),
    ]
    .join("\n")
}

fn lexical_score(text: &str, terms: &[String]) -> f64 {
    let haystack = text.to_lowercase();
    terms
        .iter()
        .map(|term| if haystack.contains(term) { 25.0 } else { 0.0 })
        .sum()
}

fn structural_score(node: &Node) -> f64 {
    match node.kind {
        NodeKind::Function | NodeKind::Method => 9.0,
        NodeKind::Struct | NodeKind::Enum | NodeKind::Trait | NodeKind::Impl => 7.0,
        NodeKind::Module | NodeKind::File => 5.0,
        _ => 1.0,
    }
}

fn terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|char: char| !char.is_ascii_alphanumeric() && char != '_')
        .filter(|term| !term.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn cosine(a: &[f64], b: &[f64]) -> f64 {
    let dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>();
    let norm_a = a.iter().map(|value| value * value).sum::<f64>().sqrt();
    let norm_b = b.iter().map(|value| value * value).sum::<f64>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    format!("{}...", &value[..limit])
}

fn is_retrievable(node: &Node) -> bool {
    matches!(
        node.kind,
        NodeKind::File
            | NodeKind::Module
            | NodeKind::Function
            | NodeKind::Method
            | NodeKind::Struct
            | NodeKind::Enum
            | NodeKind::Trait
            | NodeKind::Impl
            | NodeKind::TypeAlias
            | NodeKind::Const
            | NodeKind::Static
            | NodeKind::Macro
    )
}
