use crate::config::ModelProvider;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::{Value, json};

use super::types::{Document, RerankRequest, RerankResponse};

pub(super) fn apply_rerank(
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
