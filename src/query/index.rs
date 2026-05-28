use crate::model::{CodeGraph, Edge, Node};
use std::collections::HashMap;

pub(crate) struct QueryIndex<'a> {
    pub(crate) nodes_by_id: HashMap<&'a str, &'a Node>,
    pub(crate) outbound: HashMap<&'a str, Vec<&'a Edge>>,
    pub(crate) inbound: HashMap<&'a str, Vec<&'a Edge>>,
    pub(crate) degree: HashMap<&'a str, usize>,
}

impl<'a> QueryIndex<'a> {
    pub(crate) fn new(graph: &'a CodeGraph) -> Self {
        let mut nodes_by_id = HashMap::with_capacity(graph.nodes.len());
        let mut outbound = HashMap::<&str, Vec<&Edge>>::new();
        let mut inbound = HashMap::<&str, Vec<&Edge>>::new();
        let mut degree = HashMap::<&str, usize>::new();
        for node in &graph.nodes {
            nodes_by_id.insert(node.id.as_str(), node);
        }
        for edge in &graph.edges {
            outbound.entry(edge.from.as_str()).or_default().push(edge);
            inbound.entry(edge.to.as_str()).or_default().push(edge);
            *degree.entry(edge.from.as_str()).or_default() += edge.weight;
            *degree.entry(edge.to.as_str()).or_default() += edge.weight;
        }
        Self {
            nodes_by_id,
            outbound,
            inbound,
            degree,
        }
    }

    pub(crate) fn edges(&self, id: &str, outbound: bool) -> &[&'a Edge] {
        if outbound {
            self.outbound.get(id)
        } else {
            self.inbound.get(id)
        }
        .map(Vec::as_slice)
        .unwrap_or(&[])
    }

    pub(crate) fn node(&self, id: &str) -> Option<&'a Node> {
        self.nodes_by_id.get(id).copied()
    }

    pub(crate) fn degree(&self, id: &str) -> usize {
        self.degree.get(id).copied().unwrap_or_default()
    }
}
