/* graph-render.js — SVG rendering for nodes and edges */

CG.graphRender = {
  renderGraph() {
    const s = CG.getState();
    const svg = CG.utils.el('graph');
    if (!s.graph) {
      svg.innerHTML = '';
      if (s.status && s.status.last_event === 'failed') {
        CG.graphRender.showGraphState('error', s.status.errors ? s.status.errors.join('\n') : '未知错误');
      } else if (!s.status || s.status.last_event === 'starting') {
        CG.graphRender.showGraphState('loading');
      } else {
        CG.graphRender.showGraphState('empty');
      }
      return;
    }
    CG.graphRender.showGraphState(null);

    const rect = svg.getBoundingClientRect();
    const width = Math.max(rect.width, 600);
    const height = Math.max(rect.height, 420);
    const model = CG.graphRender.visibleModel();
    const nodes = CG.graphLayout.layoutNodes(model.nodes, model.edges, width, height);
    const edges = model.edges;
    const nodeById = new Map(nodes.map((n) => [n.id, n]));

    CG.graphRender.renderGraphNote(model, nodes, edges);

    svg.setAttribute('viewBox', `0 0 ${width} ${height}`);
    const legend = CG.utils.edgeLegend();
    svg.innerHTML = `
      <defs>
        ${legend.map((entry) => {
          const c = CG.utils.edgeColor(entry.kind);
          return `<marker id="arrow-${entry.kind}" markerWidth="12" markerHeight="10" refX="11" refY="5" orient="auto" markerUnits="userSpaceOnUse">
            <path d="M0,1.5 L10,5 L0,8.5 Z" fill="${c}" stroke="rgba(255,255,255,0.5)" stroke-width="0.8"></path>
          </marker>`;
        }).join('')}
        <marker id="arrow-default" markerWidth="12" markerHeight="10" refX="11" refY="5" orient="auto" markerUnits="userSpaceOnUse">
          <path d="M0,1.5 L10,5 L0,8.5 Z" fill="#94a3b8" stroke="rgba(255,255,255,0.5)" stroke-width="0.8"></path>
        </marker>
      </defs>
      <g id="viewport" transform="${CG.graphRender.viewTransform()}">
        ${edges.map((edge, index) => CG.graphRender.edgeSvg(edge, nodeById, index)).join('')}
        ${nodes.map((node) => CG.graphRender.nodeSvg(node)).join('')}
      </g>
    `;

    svg.querySelectorAll('[data-node-id]').forEach((nodeEl) => {
      nodeEl.addEventListener('pointerdown', CG.graphInteract.nodePointerDown);
      nodeEl.addEventListener('click', () => {
        const s = CG.getState();
        if (s.suppressClick) {
          CG.setState({ suppressClick: false });
          return;
        }
        CG.graphRender.selectNode(nodeEl.dataset.nodeId);
      });
    });
    svg.querySelectorAll('[data-edge-index]').forEach((edgeEl) => {
      edgeEl.addEventListener('click', () => {
        CG.graphRender.selectEdge(edges[Number(edgeEl.dataset.edgeIndex)]);
      });
    });

    CG.graphRender.renderEdgeLegend();
  },

  viewTransform() {
    const s = CG.getState();
    return `translate(${s.view.x} ${s.view.y}) scale(${s.view.scale})`;
  },

  applyView() {
    const viewport = document.getElementById('viewport');
    if (viewport) viewport.setAttribute('transform', CG.graphRender.viewTransform());
  },

  resetView() {
    const s = CG.getState();
    s.view = { x: 0, y: 0, scale: s.rootId ? 0.82 : 1 };
  },

  visibleModel() {
    const s = CG.getState();
    if (s.rootId) {
      return CG.graphRender.neighborhoodModel(s.rootId, Number(CG.utils.el('depthSelect').value || 2));
    }
    return CG.graphRender.overviewModel();
  },

  overviewModel() {
    const s = CG.getState();
    const kind = CG.utils.el('kindFilter').value;
    const base = s.searchItems.length ? s.searchItems : CG.graphRender.seedResults();
    const degree = CG.utils.degreeMap(s.graph.edges);
    const ids = new Set(
      base.filter((n) => !kind || n.kind === kind).slice(0, 28).map((n) => n.id)
    );
    const nodes = [...ids]
      .map((id) => s.nodesById.get(id))
      .filter(Boolean)
      .map((n) => ({ ...n, degree: degree.get(n.id) || 0, local_depth: null }));
    const edges = CG.graphRender.visibleEdges(nodes).slice(0, 80);
    return { nodes, edges, root: null, clipped: false };
  },

  neighborhoodModel(rootId, depth) {
    const s = CG.getState();
    const kind = CG.utils.el('kindFilter').value;
    const degree = CG.utils.degreeMap(s.graph.edges);
    const seen = new Map([[rootId, 0]]);
    const queue = [rootId];
    const limit = 180;

    for (let index = 0; index < queue.length && seen.size < limit; index++) {
      const current = queue[index];
      const level = seen.get(current);
      if (level >= depth) continue;
      for (const edge of s.graph.edges) {
        if (!CG.graphRender.edgePassesFilters(edge)) continue;
        if (edge.from !== current && edge.to !== current) continue;
        const other = edge.from === current ? edge.to : edge.from;
        const node = s.nodesById.get(other);
        if (!node || (kind && node.kind !== kind && other !== rootId)) continue;
        if (!seen.has(other)) {
          seen.set(other, level + 1);
          queue.push(other);
          if (seen.size >= limit) break;
        }
      }
    }

    const ids = new Set(seen.keys());
    const nodes = [...ids]
      .map((id) => s.nodesById.get(id))
      .filter(Boolean)
      .map((n) => ({ ...n, degree: degree.get(n.id) || 0, local_depth: seen.get(n.id) }));
    const edges = s.graph.edges
      .filter((edge) => ids.has(edge.from) && ids.has(edge.to) && CG.graphRender.edgePassesFilters(edge))
      .slice(0, 360);
    return { nodes, edges, root: s.nodesById.get(rootId), clipped: seen.size >= limit };
  },

  visibleEdges(nodes) {
    const s = CG.getState();
    const ids = new Set(nodes.map((n) => n.id));
    return s.graph.edges.filter((edge) => {
      if (!ids.has(edge.from) || !ids.has(edge.to)) return false;
      return CG.graphRender.edgePassesFilters(edge);
    }).slice(0, 220);
  },

  edgePassesFilters(edge) {
    const activeKinds = CG.sidebar.activeEdgeKinds();
    const s = CG.utils.el('sourceFilter');
    const c = CG.utils.el('certaintyFilter');
    if (activeKinds.size > 0 && !activeKinds.has(edge.kind)) return false;
    if (s.value && edge.source !== s.value) return false;
    if (c.value && edge.certainty !== c.value) return false;
    return true;
  },

  seedResults() {
    const s = CG.getState();
    const degree = CG.utils.degreeMap(s.graph.edges);
    return s.graph.nodes
      .filter((n) => !['project', 'crate'].includes(n.kind))
      .map((n) => ({ ...n, degree: degree.get(n.id) || 0 }))
      .sort((a, b) => b.degree - a.degree || a.qualified_name.localeCompare(b.qualified_name))
      .slice(0, 50);
  },

  edgeSvg(edge, nodeById, index) {
    const s = CG.getState();
    const from = nodeById.get(edge.from);
    const to = nodeById.get(edge.to);
    if (!from || !to) return '';
    const selected = s.selected && s.selected.edge === edge;
    const baseColor = CG.utils.edgeColor(edge.kind);
    const isPossible = edge.certainty === 'possible';
    const isSemantic = edge.source === 'rust_analyzer' || edge.source === 'mir';
    const opacity = selected ? 0.95 : isPossible ? 0.45 : 0.6;
    const strokeWidth = selected ? 2.8 : isSemantic ? 1.6 : 1.3;
    const dash = isPossible ? 'stroke-dasharray="5,3"' : '';
    const glow = isSemantic ? `filter="drop-shadow(0 0 3px ${baseColor})"` : '';
    const legendKinds = new Set(CG.utils.edgeLegend().map((e) => e.kind));
    const markerKind = legendKinds.has(edge.kind) ? edge.kind : 'default';
    // Trim line to node edges so arrow sits outside the circle
    const fromR = (Math.max(7, Math.min(18, 7 + Math.sqrt(from.degree || 0))) + 2);
    const toR = (Math.max(7, Math.min(18, 7 + Math.sqrt(to.degree || 0))) + 4);
    const dx = to.x - from.x;
    const dy = to.y - from.y;
    const dist = Math.hypot(dx, dy) || 1;
    const ux = dx / dist;
    const uy = dy / dist;
    const x1 = from.x + ux * fromR;
    const y1 = from.y + uy * fromR;
    const x2 = to.x - ux * toR;
    const y2 = to.y - uy * toR;
    const midX = (x1 + x2) / 2;
    const midY = (y1 + y2) / 2;
    return `
      <g class="edge-group" data-edge-index="${index}" data-edge-kind="${CG.utils.esc(edge.kind)}">
        <line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}"
          stroke="${baseColor}" stroke-width="${strokeWidth}" opacity="${opacity}"
          ${dash} ${glow} marker-end="url(#arrow-${markerKind})"></line>
        <text class="edge-label" x="${midX}" y="${midY}">${CG.utils.esc(edge.kind)}</text>
      </g>
    `;
  },

  nodeSvg(node) {
    const s = CG.getState();
    const selected = s.selected && s.selected.node && s.selected.node.id === node.id;
    const color = CG.utils.nodeColor(node.kind);
    const radius = Math.max(7, Math.min(18, 7 + Math.sqrt(node.degree || 0)));
    const isRoot = node.id === s.rootId;

    let glowFilter = '';
    if (isRoot) {
      glowFilter = `filter="drop-shadow(0 0 8px ${color})"`;
    } else if (selected) {
      glowFilter = `filter="drop-shadow(0 0 6px var(--accent))"`;
    }

    return `
      <g class="graph-node ${isRoot ? 'root' : ''}" data-node-id="${CG.utils.esc(node.id)}">
        <circle cx="${node.x}" cy="${node.y}" r="${isRoot ? radius + 4 : radius}" fill="${color}" ${glowFilter}
          stroke="${selected ? 'var(--accent)' : isRoot ? 'rgba(255,255,255,0.2)' : 'rgba(255,255,255,0.1)'}"
          stroke-width="${selected ? 3 : isRoot ? 3 : 1}"
          opacity="${isRoot ? 1 : 0.9}"></circle>
        ${node.showLabel ? '<text class="node-label" x="' + (node.x + radius + 5) + '" y="' + (node.y + 4) + '">' + CG.utils.esc(CG.utils.shortName(node.name || node.qualified_name)) + (node.file ? ' · ' + node.file.split('/').pop() : '') + '</text>' : ''}
      </g>
    `;
  },

  renderGraphNote(model, nodes, edges) {
    const s = CG.getState();
    const totalNodes = s.graph.stats.nodes;
    const totalEdges = s.graph.stats.edges;
    const note = CG.utils.el('graphNote');
    if (model.root) {
      note.textContent = `中心：${model.root.qualified_name || model.root.name} · 深度 ${CG.utils.el('depthSelect').value} · 当前 ${nodes.length}/${totalNodes} 节点，${edges.length}/${totalEdges} 关系${model.clipped ? ' · 已截断' : ''}`;
      return;
    }
    note.textContent = `概览模式 · 当前 ${nodes.length}/${totalNodes} 节点，${edges.length}/${totalEdges} 关系 · 从左侧选择节点进入关联图`;
  },

  renderEdgeLegend() {
    const el = CG.utils.el('edgeLegend');
    if (!el) return;
    const s = CG.getState();
    const kinds = new Set();
    for (const edge of (s.graph && s.graph.edges) || []) {
      kinds.add(edge.kind);
    }
    const legend = CG.utils.edgeLegend().filter((entry) => kinds.has(entry.kind));
    if (!legend.length) { el.innerHTML = ''; return; }
    el.innerHTML = legend.map((entry) =>
      `<span class="edge-legend-item">
        <span class="edge-legend-swatch" style="background:${CG.utils.edgeColor(entry.kind)}"></span>
        ${entry.label}
      </span>`
    ).join('');
  },

  selectNode(id) {
    const s = CG.getState();
    const node = s.nodesById.get(id);
    if (!node) return;
    if (s.rootId !== id) {
      CG.setState({ nodePositions: new Map() });
      CG.graphRender.resetView();
    }
    CG.setState({ rootId: id, selected: { node } });
    CG.details.renderDetails({ node });
    document.querySelector('.detail-drawer').classList.add('open');
    CG.sidebar.renderResults(s.searchItems.length ? s.searchItems : CG.graphRender.seedResults());
    CG.graphRender.renderGraph();
    CG.emit('node:select', { node });
  },

  selectEdge(edge) {
    CG.setState({ selected: { edge } });
    CG.details.renderDetails({ edge });
    document.querySelector('.detail-drawer').classList.add('open');
    CG.graphRender.renderGraph();
    CG.emit('edge:select', { edge });
  },

  showGraphState(state, message) {
    for (const id of ['graphLoading', 'graphEmpty', 'graphError']) {
      const el = CG.utils.el(id);
      if (el) el.classList.add('hidden');
    }
    if (!state) return;
    if (state === 'loading') CG.utils.el('graphLoading').classList.remove('hidden');
    if (state === 'empty') CG.utils.el('graphEmpty').classList.remove('hidden');
    if (state === 'error') {
      const el = CG.utils.el('graphError');
      el.classList.remove('hidden');
      if (message) CG.utils.el('graphErrorMsg').textContent = message;
    }
  },
};
