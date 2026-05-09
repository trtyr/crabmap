/* sidebar.js — Left panel: search results, filters, metrics */

CG.sidebar = {
  init() {
    // Node kind filter listener
    CG.utils.el('kindFilter').addEventListener('change', () => {
      CG.graphRender.resetView();
      CG.graphRender.renderGraph();
      CG.emit('filters:change', {});
    });
    // Source/certainty listeners
    for (const id of ['sourceFilter', 'certaintyFilter']) {
      CG.utils.el(id).addEventListener('change', () => {
        CG.graphRender.resetView();
        CG.graphRender.renderGraph();
        CG.emit('filters:change', {});
      });
    }
  },

  renderResults(items) {
    const s = CG.getState();
    const active = s.selected && s.selected.node && s.selected.node.id;
    const results = CG.utils.el('results');
    results.innerHTML = items.length ? items.map((node) => `
      <button class="row ${active === node.id ? 'active' : ''}" data-node="${CG.utils.esc(node.id)}">
        <span class="title">${CG.utils.esc(node.qualified_name || node.name)}</span>
        <span class="meta">${CG.utils.esc(node.kind)} · 连接 ${CG.utils.fmt(node.degree)}${node.file ? ` · ${CG.utils.esc(node.file)}` : ''}</span>
      </button>
    `).join('') : '<div class="empty">没有结果</div>';

    document.querySelectorAll('[data-node]').forEach((row) => {
      row.addEventListener('click', () => CG.graphRender.selectNode(row.dataset.node));
    });
    CG.emit('search:results', { items });
  },

  renderMetrics() {
    const s = CG.getState();
    const stats = s.graph.stats;
    CG.utils.el('metrics').innerHTML = [
      ['节点', stats.nodes],
      ['关系', stats.edges],
      ['文件', stats.files],
      ['符号', stats.symbols],
    ].map(([label, value]) =>
      `<div class="metric"><strong>${CG.utils.fmt(value)}</strong><span>${label}</span></div>`
    ).join('');
  },

  renderWarnings() {
    const s = CG.getState();
    const warnings = (s.graph.warnings || []).slice(0, 30);
    CG.utils.el('warnings').innerHTML = warnings.length
      ? warnings.map((item) => `<div class="row"><div class="meta">${CG.utils.esc(item)}</div></div>`).join('')
      : '<div class="empty">没有警告</div>';
  },

  fillFilters() {
    const s = CG.getState();
    CG.sidebar.setOptions(CG.utils.el('kindFilter'), '全部节点', Object.keys(s.graph.stats.by_kind || {}));
    CG.sidebar.setOptions(CG.utils.el('sourceFilter'), '全部来源', Object.keys(s.graph.stats.by_source || {}));
    CG.sidebar.setOptions(CG.utils.el('certaintyFilter'), '全部确定性', Object.keys(s.graph.stats.by_certainty || {}));
    CG.sidebar.renderEdgeFilters(Object.keys(s.graph.stats.by_edge || {}));
  },

  edgeLabel(kind) {
    const map = {
      calls: '调用', declares: '声明', uses_type: '类型使用',
      contains: '包含', imports: '导入', has_method: '方法',
      returns: '返回', module_file: '模块↔文件', implements: '实现',
      possible_dispatch: '可能分发',
    };
    return map[kind] || kind;
  },

  renderEdgeFilters(edgeKinds) {
    const el = CG.utils.el('edgeFilters');
    if (!el) return;
    const activeKinds = CG.sidebar.loadEdgeKinds();
    // Default: only calls active if nothing stored
    if (!activeKinds.size && edgeKinds.includes('calls')) {
      activeKinds.add('calls');
    }
    el.innerHTML = edgeKinds.sort().map((kind) => {
      const active = activeKinds.has(kind);
      const color = CG.utils.edgeColor(kind);
      return `<button class="edge-filter-pill${active ? ' active' : ''}" data-kind="${CG.utils.esc(kind)}">
        <span class="edge-filter-dot" style="background:${color}"></span>
        ${CG.sidebar.edgeLabel(kind)}
      </button>`;
    }).join('');
    el.querySelectorAll('.edge-filter-pill').forEach((pill) => {
      pill.addEventListener('click', () => {
        pill.classList.toggle('active');
        CG.sidebar.saveEdgeKinds();
        CG.graphRender.resetView();
        CG.graphRender.renderGraph();
        CG.emit('filters:change', {});
      });
    });
  },

  activeEdgeKinds() {
    const el = CG.utils.el('edgeFilters');
    if (!el) return new Set();
    return new Set(
      [...el.querySelectorAll('.edge-filter-pill.active')]
        .map((pill) => pill.dataset.kind)
        .filter(Boolean)
    );
  },

  saveEdgeKinds() {
    try {
      localStorage.setItem('cg-edge-kinds',
        JSON.stringify([...CG.sidebar.activeEdgeKinds()]));
    } catch (_) { /* ignore */ }
  },

  loadEdgeKinds() {
    try {
      const raw = localStorage.getItem('cg-edge-kinds');
      return raw ? new Set(JSON.parse(raw)) : new Set();
    } catch (_) { return new Set(); }
  },

  setOptions(select, first, values) {
    const previous = select.value;
    select.innerHTML = [`<option value="">${first}</option>`]
      .concat(values.sort().map((value) => `<option value="${CG.utils.esc(value)}">${CG.utils.esc(value)}</option>`))
      .join('');
    select.value = values.includes(previous) ? previous : '';
  },
};
