/* graph-interact.js — Drag, zoom, select handlers */

CG.graphInteract = {
  init() {
    const svg = CG.utils.el('graph');

    svg.addEventListener('pointerdown', CG.graphInteract.canvasPointerDown);
    window.addEventListener('pointermove', CG.graphInteract.pointerMove);
    window.addEventListener('pointerup', CG.graphInteract.pointerUp);
    window.addEventListener('resize', () => CG.graphRender.renderGraph());

    svg.addEventListener('wheel', (event) => {
      event.preventDefault();
      CG.graphInteract.zoomAt(
        event.deltaY < 0 ? 1.12 : 1 / 1.12,
        event.clientX,
        event.clientY
      );
    }, { passive: false });

    CG.utils.el('zoomInButton').addEventListener('click', () => CG.graphInteract.zoomAt(1.2));
    CG.utils.el('zoomOutButton').addEventListener('click', () => CG.graphInteract.zoomAt(1 / 1.2));
    CG.utils.el('fitButton').addEventListener('click', () => {
      CG.graphRender.resetView();
      CG.graphRender.renderGraph();
    });
  },

  nodePointerDown(event) {
    event.stopPropagation();
    const node = event.currentTarget;
    const id = node.dataset.nodeId;
    const point = CG.graphInteract.screenToGraph(event.clientX, event.clientY);
    const s = CG.getState();
    const current = s.nodePositions.get(id) || point;
    CG.setState({
      drag: {
        type: 'node',
        id,
        moved: false,
        startX: point.x,
        startY: point.y,
        nodeX: current.x,
        nodeY: current.y,
      },
    });
    node.classList.add('dragging');
    node.setPointerCapture(event.pointerId);
  },

  canvasPointerDown(event) {
    if (event.target.closest('[data-node-id]') || event.target.closest('[data-edge-index]')) return;
    const s = CG.getState();
    CG.utils.el('graph').classList.add('dragging');
    CG.setState({
      drag: {
        type: 'pan',
        moved: false,
        x: event.clientX,
        y: event.clientY,
        viewX: s.view.x,
        viewY: s.view.y,
      },
    });
    CG.utils.el('graph').setPointerCapture(event.pointerId);
  },

  pointerMove(event) {
    const s = CG.getState();
    if (!s.drag) return;
    s.drag.moved = true;
    if (s.drag.type === 'pan') {
      s.view.x = s.drag.viewX + event.clientX - s.drag.x;
      s.view.y = s.drag.viewY + event.clientY - s.drag.y;
      CG.graphRender.applyView();
      return;
    }
    const point = CG.graphInteract.screenToGraph(event.clientX, event.clientY);
    s.nodePositions.set(s.drag.id, {
      x: s.drag.nodeX + point.x - s.drag.startX,
      y: s.drag.nodeY + point.y - s.drag.startY,
    });
    CG.graphRender.renderGraph();
  },

  pointerUp() {
    document.querySelectorAll('.dragging').forEach((item) => item.classList.remove('dragging'));
    const s = CG.getState();
    const suppressClick = Boolean(s.drag && s.drag.type === 'node' && s.drag.moved);
    CG.setState({ drag: null, suppressClick });
  },

  zoomAt(factor, clientX, clientY) {
    const svg = CG.utils.el('graph');
    const rect = svg.getBoundingClientRect();
    const s = CG.getState();
    const x = clientX === undefined ? rect.left + rect.width / 2 : clientX;
    const y = clientY === undefined ? rect.top + rect.height / 2 : clientY;
    const before = CG.graphInteract.screenToGraph(x, y);
    s.view.scale = Math.max(0.25, Math.min(4, s.view.scale * factor));
    const after = CG.graphInteract.screenToGraph(x, y);
    s.view.x += (after.x - before.x) * s.view.scale;
    s.view.y += (after.y - before.y) * s.view.scale;
    CG.graphRender.applyView();
    CG.emit('view:change', s.view);
  },

  screenToGraph(clientX, clientY) {
    const svg = CG.utils.el('graph');
    const rect = svg.getBoundingClientRect();
    const s = CG.getState();
    return {
      x: (clientX - rect.left - s.view.x) / s.view.scale,
      y: (clientY - rect.top - s.view.y) / s.view.scale,
    };
  },
};
