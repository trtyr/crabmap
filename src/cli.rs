use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "ferrimind",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("GIT_DESC"), " ", env!("BUILD_DATE"), ")"),
    about = "Rust code knowledge graph for AI navigation"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Args, Debug, Clone)]
pub struct IndexOpts {
    #[arg(long)]
    pub all: bool,
    #[arg(long = "no-tests", default_value_t = false)]
    pub no_tests: bool,
    #[arg(long)]
    pub features: Vec<String>,
    #[arg(long)]
    pub all_features: bool,
    #[arg(long)]
    pub no_default_features: bool,
    #[arg(long)]
    pub target: Option<String>,
    #[arg(long = "no-semantic", default_value_t = false)]
    pub no_semantic: bool,
    #[arg(long, default_value_t = 200)]
    pub semantic_limit: usize,
    #[arg(long)]
    pub mir: bool,
    #[arg(long, default_value_t = 500)]
    pub mir_limit: usize,
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 4)]
    pub max_depth: usize,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Index(IndexArgs),
    Serve(ServeArgs),
    #[command(subcommand)]
    Query(QueryCmd),
    #[command(subcommand)]
    Nav(NavCmd),
    #[command(subcommand)]
    Analyze(AnalyzeCmd),
    Config(ConfigArgs),
}

#[derive(Subcommand, Debug)]
pub enum QueryCmd {
    Stats(GraphArg),
    Summary(SummaryArgs),
    Symbols(SymbolsArgs),
    Symbol(SymbolArg),
    File(FileArg),
    Module(ModuleArg),
    Callees(WalkArg),
    Callers(WalkArg),
    Impact(WalkArg),
    Search(SearchArgs),
    Path(PathArgs),
    Export(ExportArgs),
}

#[derive(Subcommand, Debug)]
pub enum NavCmd {
    Guide(GuideArgs),
    Entries(ListArg),
    Clusters(ListArg),
    Quality(GraphArg),
    Health(ListArg),
    Report(ReportArgs),
    Map(MapArgs),
    Ask(AskArgs),
    Retrieve(RetrieveArgs),
}

#[derive(Subcommand, Debug)]
pub enum AnalyzeCmd {
    Deps(DepsArgs),
    Fanout(ListArg),
    Tests(TestsArgs),
    Hotspots(GitArgs),
    Diff(DiffArgs),
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    #[arg(default_value = ".")]
    pub project: PathBuf,
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long, default_value_t = 7878)]
    pub port: u16,
    #[arg(long)]
    pub graph: Option<PathBuf>,
    #[arg(long)]
    pub watch: bool,
    #[command(flatten)]
    pub index: IndexOpts,
}

#[derive(Args, Debug)]
pub struct IndexArgs {
    #[arg(default_value = ".")]
    pub project: PathBuf,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[command(flatten)]
    pub index: IndexOpts,
}

#[derive(Args, Debug)]
pub struct GraphArg {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
}

#[derive(Args, Debug)]
pub struct SummaryArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 30)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct SymbolsArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub kind: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct SymbolArg {
    pub name: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
}

#[derive(Args, Debug)]
pub struct FileArg {
    pub path: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
}

#[derive(Args, Debug)]
pub struct ModuleArg {
    pub name: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
}

#[derive(Args, Debug)]
pub struct WalkArg {
    pub name: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 2)]
    pub depth: usize,
    #[arg(long, default_value_t = 100)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    pub query: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct ListArg {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct GuideArgs {
    pub query: Option<String>,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 2)]
    pub depth: usize,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct ReportArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long)]
    pub output: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct MapArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 8000)]
    pub budget: usize,
}

#[derive(Args, Debug)]
pub struct DiffArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[arg(long, default_value = "HEAD")]
    pub base: String,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct DepsArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long)]
    pub from: Option<String>,
    #[arg(long)]
    pub to: Option<String>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct TestsArgs {
    pub symbol: Option<String>,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 50)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct GitArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long)]
    pub project: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct AskArgs {
    pub question: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 2)]
    pub depth: usize,
    #[arg(long, default_value_t = 20)]
    pub limit: usize,
    #[arg(long, default_value_t = 2000)]
    pub max_tokens: usize,
}

#[derive(Args, Debug)]
pub struct RetrieveArgs {
    pub query: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 10)]
    pub limit: usize,
    #[arg(long, default_value_t = 80)]
    pub candidates: usize,
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[arg(long)]
    pub api_key: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    #[arg(long)]
    pub api_url: Option<String>,
    #[arg(long)]
    pub embedding_key: Option<String>,
    #[arg(long)]
    pub embedding_model: Option<String>,
    #[arg(long)]
    pub embedding_url: Option<String>,
    #[arg(long)]
    pub rerank_key: Option<String>,
    #[arg(long)]
    pub rerank_model: Option<String>,
    #[arg(long)]
    pub rerank_url: Option<String>,
}

#[derive(Args, Debug)]
pub struct PathArgs {
    pub from: String,
    pub to: String,
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, default_value_t = 6)]
    pub depth: usize,
}

#[derive(Args, Debug)]
pub struct ExportArgs {
    #[arg(long)]
    pub graph: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = ExportFormat::Json)]
    pub format: ExportFormat,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExportFormat {
    Json,
    Dot,
    Mermaid,
}
