/* utils.js — Escape, format, helpers */

CG.utils = {
  el(id) {
    return document.getElementById(id);
  },

  fmt(value) {
    return new Intl.NumberFormat().format(value || 0);
  },

  esc(value) {
    return String(value ?? '').replace(/[&<>"']/g, (char) => ({
      '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
    })[char]);
  },

  shortName(value) {
    const text = String(value || '');
    return text.length > 34 ? `${text.slice(0, 31)}...` : text;
  },

  degreeMap(edges) {
    const result = new Map();
    for (const edge of edges || []) {
      result.set(edge.from, (result.get(edge.from) || 0) + (edge.weight || 1));
      result.set(edge.to, (result.get(edge.to) || 0) + (edge.weight || 1));
    }
    return result;
  },

  statusText(value) {
    if (value === 'ready') return '就绪';
    if (value === 'failed') return '失败';
    if (value === 'starting') return '启动中';
    return value;
  },

  nodeColor(kind) {
    const colors = {
      file: '#94a3b8',
      module: '#38bdf8',
      function: '#4ade80',
      method: '#4ade80',
      struct: '#fbbf24',
      enum: '#fbbf24',
      enum_member: '#f59e0b',
      trait: '#fbbf24',
      impl: '#a78bfa',
      field: '#fb923c',
      const: '#2dd4bf',
    };
    return colors[kind] || '#64748b';
  },

  edgeColor(kind) {
    const colors = {
      calls: '#60a5fa',
      declares: '#f59e0b',
      uses_type: '#c084fc',
      contains: '#34d399',
      imports: '#2dd4bf',
      has_method: '#f472b6',
      returns: '#fb923c',
      module_file: '#94a3b8',
      implements: '#22d3ee',
      possible_dispatch: '#f87171',
    };
    return colors[kind] || '#64748b';
  },

  edgeLegend() {
    return [
      { kind: 'calls', label: '调用' },
      { kind: 'declares', label: '声明' },
      { kind: 'uses_type', label: '类型使用' },
      { kind: 'contains', label: '包含' },
      { kind: 'imports', label: '导入' },
      { kind: 'has_method', label: '方法' },
      { kind: 'returns', label: '返回' },
      { kind: 'module_file', label: '模块文件' },
      { kind: 'implements', label: '实现' },
      { kind: 'possible_dispatch', label: '可能分发' },
    ];
  },
};
