use crate::model::{CodeGraph, Node};
use anyhow::Result;

pub(crate) fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.chars().collect::<Vec<_>>();
    let b = b.chars().collect::<Vec<_>>();
    let n = a.len();
    let m = b.len();
    let mut prev = (0..=m).collect::<Vec<_>>();
    let mut curr = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

pub(crate) fn suggest(query: &str, candidates: &[&str], limit: usize) -> String {
    let mut scored: Vec<(&str, usize)> = candidates
        .iter()
        .map(|c| (*c, levenshtein(query, c)))
        .collect();
    scored.sort_by_key(|(_, d)| *d);
    let suggestions: Vec<_> = scored
        .iter()
        .take(limit)
        .filter(|(_, d)| *d < query.len().max(5))
        .map(|(name, _)| format!("  • {name}"))
        .collect();
    if suggestions.is_empty() {
        String::new()
    } else {
        format!("\nDid you mean?\n{}", suggestions.join("\n"))
    }
}

pub(crate) fn find_nodes<'a>(graph: &'a CodeGraph, name: &str) -> Vec<&'a Node> {
    let exact = graph
        .nodes
        .iter()
        .filter(|node| {
            node.id == name
                || node
                    .id
                    .strip_suffix(|ch: char| ch == '#' || ch.is_ascii_digit())
                    .is_some_and(|base| base == name)
                || node.qualified_name == name
        })
        .collect::<Vec<_>>();
    if !exact.is_empty() {
        return exact;
    }

    let by_name = graph
        .nodes
        .iter()
        .filter(|node| node.name == name)
        .collect::<Vec<_>>();
    if !by_name.is_empty() {
        return by_name;
    }

    let suffix = format!("::{name}");
    graph
        .nodes
        .iter()
        .filter(|node| node.qualified_name.ends_with(&suffix))
        .collect()
}

pub(crate) fn require_unique_node<'a>(
    graph: &'a CodeGraph,
    name: &str,
    label: &str,
) -> Result<&'a Node> {
    let matches = find_nodes(graph, name);
    if matches.is_empty() {
        let names: Vec<&str> = graph.nodes.iter().map(|n| n.name.as_str()).collect();
        anyhow::bail!("{label} `{name}` not found{}", suggest(name, &names, 3));
    }
    if matches.len() > 1 {
        let names = matches
            .iter()
            .map(|node| node.qualified_name.as_str())
            .collect::<Vec<_>>();
        anyhow::bail!(
            "{label} `{name}` is ambiguous, matches: {}",
            names.join(", ")
        );
    }
    Ok(matches[0])
}
