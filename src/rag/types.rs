use crate::model::Node;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Document<'a> {
    pub(super) node: &'a Node,
    pub(super) text: String,
    pub(super) lexical: f64,
    pub(super) embedding: Option<f64>,
    pub(super) rerank: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct RetrievedRoot {
    pub id: String,
}

#[derive(Serialize)]
pub(super) struct EmbeddingRequest {
    pub(super) model: String,
    pub(super) input: Vec<String>,
}

#[derive(Deserialize)]
pub(super) struct EmbeddingResponse {
    pub(super) data: Vec<EmbeddingItem>,
    #[serde(default)]
    pub(super) usage: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct EmbeddingItem {
    pub(super) embedding: Vec<f64>,
    #[serde(default)]
    pub(super) index: Option<usize>,
}

#[derive(Serialize)]
pub(super) struct RerankRequest {
    pub(super) model: String,
    pub(super) query: String,
    pub(super) documents: Vec<String>,
    pub(super) top_n: usize,
    pub(super) return_documents: bool,
}

#[derive(Deserialize)]
pub(super) struct RerankResponse {
    #[serde(default)]
    pub(super) results: Vec<RerankItem>,
    #[serde(default)]
    pub(super) data: Vec<RerankItem>,
    #[serde(default)]
    pub(super) usage: Option<Value>,
}

#[derive(Deserialize)]
pub(super) struct RerankItem {
    pub(super) index: usize,
    #[serde(default)]
    pub(super) relevance_score: Option<f64>,
    #[serde(default)]
    pub(super) score: Option<f64>,
}
