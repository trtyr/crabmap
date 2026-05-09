/* details.js — Right drawer: node/edge details */

CG.details = {
  init() {
    CG.utils.el('closeDetailsButton').addEventListener('click', () => {
      document.querySelector('.detail-drawer').classList.remove('open');
    });
  },

  renderDetails(selection) {
    const details = CG.utils.el('details');
    if (selection.node) {
      const node = selection.node;
      details.className = 'section';
      details.innerHTML = `
        <dl class="kv">
          <dt>类型</dt><dd>${CG.utils.esc(node.kind)}</dd>
          <dt>名称</dt><dd>${CG.utils.esc(node.name)}</dd>
          <dt>全名</dt><dd>${CG.utils.esc(node.qualified_name)}</dd>
          <dt>文件</dt><dd>${CG.utils.esc(node.file || '')}</dd>
          <dt>位置</dt><dd>${node.range ? `${node.range.start_line}-${node.range.end_line}` : ''}</dd>
          <dt>连接</dt><dd>${CG.utils.fmt(node.degree)}</dd>
        </dl>
        ${node.signature ? `<pre>${CG.utils.esc(node.signature)}</pre>` : ''}
        ${node.docs ? `<pre>${CG.utils.esc(node.docs)}</pre>` : ''}
        ${CG.details.fileSymbols(node)}
      `;
      // Attach click handlers for file symbol rows
      details.querySelectorAll('[data-node]').forEach((row) => {
        row.addEventListener('click', () => CG.graphRender.selectNode(row.dataset.node));
      });
      return;
    }
    const edge = selection.edge;
    const s = CG.getState();
    details.className = 'section';
    details.innerHTML = `
      <dl class="kv">
        <dt>关系</dt><dd>${CG.utils.esc(edge.kind)}</dd>
        <dt>来源</dt><dd>${CG.utils.esc(edge.source)}</dd>
        <dt>确定性</dt><dd>${CG.utils.esc(edge.certainty)}</dd>
        <dt>权重</dt><dd>${CG.utils.fmt(edge.weight)}</dd>
        <dt>起点</dt><dd>${CG.utils.esc(s.nodesById.get(edge.from)?.qualified_name || edge.from)}</dd>
        <dt>终点</dt><dd>${CG.utils.esc(s.nodesById.get(edge.to)?.qualified_name || edge.to)}</dd>
        <dt>证据</dt><dd>${edge.evidence ? `${CG.utils.esc(edge.evidence.file)}:${edge.evidence.line}` : ''}</dd>
      </dl>
    `;
  },

  // Show all symbols declared in a file
  fileSymbols(node) {
    if (node.kind !== 'file') return '';
    const s = CG.getState();
    const symbols = [...s.nodesById.values()]
      .filter(n => n.file === node.name && n.kind !== 'file' && n.kind !== 'module' && n.kind !== 'project' && n.kind !== 'crate')
      .sort((a, b) => (a.range?.start_line || 0) - (b.range?.start_line || 0));
    if (!symbols.length) return '<div class="empty">未找到声明</div>';
    return `
      <h2>声明 (${symbols.length})</h2>
      <div class="list">
        ${symbols.map(sym => `
          <button class="row" data-node="${CG.utils.esc(sym.id)}">
            <span class="title"><span class="dot" style="background:${CG.utils.nodeColor(sym.kind)};width:7px;height:7px;display:inline-block;border-radius:999px;margin-right:6px"></span>${CG.utils.esc(sym.name)}</span>
            <span class="meta">${CG.utils.esc(sym.kind)}${sym.signature ? ` · ${CG.utils.esc(sym.signature.slice(0, 80))}${sym.signature.length > 80 ? '...' : ''}` : ''}${sym.range ? ` · L${sym.range.start_line}` : ''}</span>
          </button>
        `).join('')}
      </div>
    `;
  },
};
