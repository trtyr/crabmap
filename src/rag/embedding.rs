use crate::config::ModelProvider;
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde_json::{Value, json};

use super::helpers::cosine;
use super::types::{Document, EmbeddingRequest, EmbeddingResponse};

pub(super) fn apply_embedding(
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
