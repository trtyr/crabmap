use crate::config::{CodegraphConfig, ModelProvider};
use crate::model::CodeGraph;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::{Value, json};
use std::time::Duration;

use super::embedding::apply_embedding;
use super::helpers::{document_value, sort_docs};
use super::lexical::lexical_candidates;
use super::rerank::apply_rerank;
use super::types::RetrievedRoot;

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

pub(super) fn configured_provider(provider: Option<&ModelProvider>) -> Option<&ModelProvider> {
    provider.and_then(|provider| {
        provider
            .api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty())
            .then_some(provider)
    })
}
