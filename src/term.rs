/// Minimal ANSI terminal colors. No dependencies.
pub(crate) struct Style;

impl Style {
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const CYAN: &str = "\x1b[36m";
    pub const BOLD: &str = "\x1b[1m";
    pub const RESET: &str = "\x1b[0m";
}

pub(crate) fn red(text: &str) -> String {
    format!("{}{}{}", Style::RED, text, Style::RESET)
}

pub(crate) fn green(text: &str) -> String {
    format!("{}{}{}", Style::GREEN, text, Style::RESET)
}

pub(crate) fn yellow(text: &str) -> String {
    format!("{}{}{}", Style::YELLOW, text, Style::RESET)
}

pub(crate) fn cyan(text: &str) -> String {
    format!("{}{}{}", Style::CYAN, text, Style::RESET)
}

pub(crate) fn bold(text: &str) -> String {
    format!("{}{}{}", Style::BOLD, text, Style::RESET)
}

/// Returns true if stderr is a terminal (skip colors when piping)
pub(crate) fn use_colors() -> bool {
    std::io::IsTerminal::is_terminal(&std::io::stderr())
}
