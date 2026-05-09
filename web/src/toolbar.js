/* toolbar.js — Top controls: search, depth, zoom, rebuild, status */

CG.toolbar = {
  init() {
    CG.utils.el('searchButton').addEventListener('click', CG.toolbar.search);
    CG.utils.el('search').addEventListener('keydown', (event) => {
      if (event.key === 'Enter') CG.toolbar.search();
    });

    CG.utils.el('depthSelect').addEventListener('change', () => {
      const s = CG.getState();
      CG.setState({ nodePositions: new Map() });
      CG.graphRender.resetView();
      CG.graphRender.renderGraph();
    });

    CG.utils.el('reindexButton').addEventListener('click', CG.toolbar.reindex);
  },

  renderStatus() {
    const s = CG.getState();
    CG.utils.el('project').textContent = s.status.project;
    CG.utils.el('graphPath').textContent = s.status.graph_path;
    CG.utils.el('latency').textContent = `${CG.utils.fmt(s.status.last_index_ms)} ms`;
    CG.utils.el('state').textContent = s.status.indexing ? '索引中' : CG.utils.statusText(s.status.last_event);
    CG.utils.el('state').className = `pill ${s.status.indexing ? 'indexing' : s.status.last_event}`;
    CG.utils.el('reindexButton').disabled = s.status.indexing;
  },

  async search() {
    const query = CG.utils.el('search').value.trim();
    const result = await CG.api.search(query, 50);
    const s = CG.getState();
    CG.setState({ searchItems: result.items || [], rootId: null });
    CG.sidebar.renderResults(s.searchItems);
    CG.graphRender.resetView();
    CG.graphRender.renderGraph();
  },

  async reindex() {
    CG.utils.el('reindexButton').disabled = true;
    await CG.api.reindex();
    CG.setState({ graph: null });
    await CG.toolbar.refreshStatus();
  },

  async refreshStatus() {
    try {
      const status = await CG.api.status();
      CG.setState({ status });
      CG.toolbar.renderStatus();
      if (status.last_event === 'ready' && !CG.getState().graph) {
        await CG.toolbar.loadGraph();
      }
      if (status.last_event === 'failed') {
        CG.setState({ graph: null });
        CG.graphRender.renderGraph();
      }
    } catch (error) {
      CG.utils.el('state').textContent = error.message;
      CG.utils.el('state').className = 'pill failed';
      CG.setState({ graph: null });
      CG.graphRender.renderGraph();
    }
  },

  async loadGraph() {
    try {
      const graph = await CG.api.graph();
      if (graph.error) {
        CG.setState({ graph: null });
        CG.graphRender.renderGraph();
        return;
      }
      const s = CG.getState();
      CG.setState({
        graph,
        nodesById: new Map(graph.nodes.map((node) => [node.id, node])),
        nodePositions: new Map(),
        rootId: null,
        selected: null,
      });
      CG.sidebar.fillFilters();
      CG.sidebar.renderMetrics();
      CG.sidebar.renderWarnings();
      CG.sidebar.renderResults(CG.graphRender.seedResults());

      // Auto-select run as entry point
      const runNode = [...CG.getState().nodesById.values()].find(
        n => n.qualified_name === 'ferrimind::run'
      );
      if (runNode) {
        CG.setState({ rootId: runNode.id });
      }

      CG.graphRender.renderGraph();
      CG.emit('graph:loaded', { graph });
    } catch (error) {
      CG.setState({ graph: null });
      CG.graphRender.renderGraph();
      CG.utils.el('state').textContent = error.message;
      CG.utils.el('state').className = 'pill failed';
    }
  },
};
