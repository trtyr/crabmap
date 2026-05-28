use super::config::ServeConfig;
use crate::model::CodeGraph;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

pub(crate) struct AppState {
    pub(crate) config: ServeConfig,
    pub(crate) graph: Option<CodeGraph>,
    pub(crate) graph_gz: Option<Vec<u8>>,
    pub(crate) status: Status,
    pub(crate) last_mtimes: BTreeMap<PathBuf, SystemTime>,
}

#[derive(Clone, Serialize)]
pub(crate) struct Status {
    pub(crate) project: String,
    pub(crate) graph_path: String,
    pub(crate) indexing: bool,
    pub(crate) last_event: String,
    pub(crate) last_index_ms: u128,
    pub(crate) errors: Vec<String>,
}
