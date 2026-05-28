use crate::cli::ServeArgs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Clone)]
pub(crate) struct ServeConfig {
    pub(crate) project: PathBuf,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) graph: Option<PathBuf>,
    pub(crate) watch_seconds: Option<Duration>,
    pub(crate) include_tests: bool,
    pub(crate) features: Vec<String>,
    pub(crate) all_features: bool,
    pub(crate) no_default_features: bool,
    pub(crate) target: Option<String>,
    pub(crate) profile: String,
    pub(crate) semantic: bool,
    pub(crate) semantic_limit: usize,
    pub(crate) mir: bool,
    pub(crate) mir_limit: usize,
}

impl ServeConfig {
    pub(crate) fn host(&self) -> &str {
        &self.host
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    pub(crate) fn watch_seconds(&self) -> Option<Duration> {
        self.watch_seconds
    }
}

impl From<ServeArgs> for ServeConfig {
    fn from(args: ServeArgs) -> Self {
        Self {
            project: args.project,
            host: args.host,
            port: args.port,
            graph: args.graph,
            watch_seconds: args.watch.then(|| Duration::from_secs(2)),
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
        }
    }
}
