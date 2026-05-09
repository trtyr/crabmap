use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

const DEFAULT_API_URL: &str = "https://api.minimaxi.com/anthropic/v1/messages";
const DEFAULT_MODEL: &str = "MiniMax-M2.7-highspeed";
const DEFAULT_EMBEDDING_URL: &str = "https://api.siliconflow.cn/v1/embeddings";
const DEFAULT_EMBEDDING_MODEL: &str = "Qwen/Qwen3-Embedding-8B";
const DEFAULT_RERANK_URL: &str = "https://api.siliconflow.cn/v1/rerank";
const DEFAULT_RERANK_MODEL: &str = "Qwen/Qwen3-Reranker-8B";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CodegraphConfig {
    pub api_url: String,
    pub model: String,
    pub api_key: Option<String>,
    #[serde(default)]
    pub embedding: Option<ModelProvider>,
    #[serde(default)]
    pub rerank: Option<ModelProvider>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelProvider {
    pub api_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

impl Default for CodegraphConfig {
    fn default() -> Self {
        Self {
            api_url: DEFAULT_API_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            api_key: None,
            embedding: Some(ModelProvider {
                api_url: DEFAULT_EMBEDDING_URL.to_string(),
                model: DEFAULT_EMBEDDING_MODEL.to_string(),
                api_key: None,
            }),
            rerank: Some(ModelProvider {
                api_url: DEFAULT_RERANK_URL.to_string(),
                model: DEFAULT_RERANK_MODEL.to_string(),
                api_key: None,
            }),
        }
    }
}

pub fn path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set; cannot locate ferrimind config")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("ferrimind")
        .join("config.json"))
}

pub fn load() -> Result<CodegraphConfig> {
    let path = path()?;
    if !path.exists() {
        return Ok(CodegraphConfig::default());
    }
    Ok(serde_json::from_slice(
        &std::fs::read(&path)
            .with_context(|| format!("failed to read ferrimind config at {}", path.display()))?,
    )?)
}

pub fn save(config: &CodegraphConfig) -> Result<PathBuf> {
    let path = path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(&path, serde_json::to_vec_pretty(config)?)
        .with_context(|| format!("failed to write {}", path.display()))?;
    set_private_permissions(&path)?;
    Ok(path)
}

pub fn update(
    api_key: Option<String>,
    model: Option<String>,
    api_url: Option<String>,
    embedding_key: Option<String>,
    embedding_model: Option<String>,
    embedding_url: Option<String>,
    rerank_key: Option<String>,
    rerank_model: Option<String>,
    rerank_url: Option<String>,
) -> Result<Value> {
    let mut config = load()?;
    if let Some(api_key) = api_key {
        config.api_key = Some(api_key);
    }
    if let Some(model) = model {
        config.model = model;
    }
    if let Some(api_url) = api_url {
        config.api_url = api_url;
    }
    if embedding_key.is_some() || embedding_model.is_some() || embedding_url.is_some() {
        let mut embedding = config.embedding.unwrap_or_else(default_embedding);
        if let Some(api_key) = embedding_key {
            embedding.api_key = Some(api_key);
        }
        if let Some(model) = embedding_model {
            embedding.model = model;
        }
        if let Some(api_url) = embedding_url {
            embedding.api_url = api_url;
        }
        config.embedding = Some(embedding);
    }
    if rerank_key.is_some() || rerank_model.is_some() || rerank_url.is_some() {
        let mut rerank = config.rerank.unwrap_or_else(default_rerank);
        if let Some(api_key) = rerank_key {
            rerank.api_key = Some(api_key);
        }
        if let Some(model) = rerank_model {
            rerank.model = model;
        }
        if let Some(api_url) = rerank_url {
            rerank.api_url = api_url;
        }
        config.rerank = Some(rerank);
    }
    let path = save(&config)?;
    Ok(json!({
        "kind": "config",
        "path": path,
        "config": redacted(&config)
    }))
}

pub fn show() -> Result<Value> {
    Ok(json!({
        "kind": "config",
        "path": path()?,
        "config": redacted(&load()?)
    }))
}

pub fn redacted(config: &CodegraphConfig) -> Value {
    json!({
        "api_url": config.api_url,
        "model": config.model,
        "api_key": config.api_key.as_ref().map(|key| redact_key(key)),
        "embedding": config.embedding.as_ref().map(redacted_provider),
        "rerank": config.rerank.as_ref().map(redacted_provider)
    })
}

fn default_embedding() -> ModelProvider {
    ModelProvider {
        api_url: DEFAULT_EMBEDDING_URL.to_string(),
        model: DEFAULT_EMBEDDING_MODEL.to_string(),
        api_key: None,
    }
}

fn default_rerank() -> ModelProvider {
    ModelProvider {
        api_url: DEFAULT_RERANK_URL.to_string(),
        model: DEFAULT_RERANK_MODEL.to_string(),
        api_key: None,
    }
}

fn redacted_provider(provider: &ModelProvider) -> Value {
    json!({
        "api_url": provider.api_url,
        "model": provider.model,
        "api_key": provider.api_key.as_ref().map(|key| redact_key(key))
    })
}

fn redact_key(key: &str) -> String {
    if key.len() <= 10 {
        "***".to_string()
    } else {
        format!("{}...{}", &key[..6], &key[key.len() - 4..])
    }
}

#[cfg(unix)]
fn set_private_permissions(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("failed to chmod 600 {}", path.display()))
}

#[cfg(not(unix))]
fn set_private_permissions(_: &std::path::Path) -> Result<()> {
    Ok(())
}
