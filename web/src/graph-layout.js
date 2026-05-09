/* graph-layout.js — Seeded positions + force-directed relaxation */

CG.graphLayout = {
  seededPoint(id, width, height, index) {
    const hash = [...String(id)].reduce(
      (value, char) => (value * 31 + char.charCodeAt(0)) >>> 0,
      index + 17
    );
    const x = width * (0.08 + ((hash % 1000) / 1000) * 0.84);
    const y = height * (0.08 + (((hash >>> 10) % 1000) / 1000) * 0.84);
    return { x, y };
  },

  layoutNodes(nodes, edges, width, height) {
    const s = CG.getState();
    const centerX = width / 2;
    const centerY = height / 2;

    const positioned = nodes.map((node, index) => {
      const cached = s.nodePositions.get(node.id);
      const seeded = CG.graphLayout.seededPoint(node.id, width, height, index);
      const item = {
        ...node,
        x: cached ? cached.x : seeded.x,
        y: cached ? cached.y : seeded.y,
        vx: 0,
        vy: 0,
        showLabel: true,
      };
      if (node.id === s.rootId) {
        item.x = centerX;
        item.y = centerY;
      }
      return item;
    });

    CG.graphLayout.relaxLayout(positioned, edges, width, height);

    for (const node of positioned) {
      s.nodePositions.set(node.id, { x: node.x, y: node.y });
    }
    return positioned;
  },

  relaxLayout(nodes, edges, width, height) {
    const s = CG.getState();
    const byId = new Map(nodes.map((node) => [node.id, node]));
    const centerX = width / 2;
    const centerY = height / 2;
    const iterations = nodes.length > 120 ? 90 : 150;
    const density = Math.min(1.6, Math.max(1, Math.sqrt(nodes.length / 45)));
    const spread = s.rootId ? 1.55 * density : 1.25;

    for (let tick = 0; tick < iterations; tick++) {
      // Repulsion between all node pairs
      for (let i = 0; i < nodes.length; i++) {
        for (let j = i + 1; j < nodes.length; j++) {
          const a = nodes[i];
          const b = nodes[j];
          const dx = b.x - a.x || 0.01;
          const dy = b.y - a.y || 0.01;
          const distance = Math.max(24, Math.hypot(dx, dy));
          const force = Math.min(12, (2800 * spread) / (distance * distance));
          const fx = (dx / distance) * force;
          const fy = (dy / distance) * force;
          a.vx -= fx;
          a.vy -= fy;
          b.vx += fx;
          b.vy += fy;
        }
      }

      // Attraction along edges
      for (const edge of edges) {
        const from = byId.get(edge.from);
        const to = byId.get(edge.to);
        if (!from || !to) continue;
        const dx = to.x - from.x || 0.01;
        const dy = to.y - from.y || 0.01;
        const distance = Math.max(1, Math.hypot(dx, dy));
        const level = Math.max(from.local_depth || 0, to.local_depth || 0);
        const base = edge.kind === 'declares' || edge.kind === 'module_file' ? 155 : 210;
        const ideal = (base + level * 34) * spread;
        const force = (distance - ideal) * 0.0065;
        const fx = (dx / distance) * force;
        const fy = (dy / distance) * force;
        from.vx += fx;
        from.vy += fy;
        to.vx -= fx;
        to.vy -= fy;
      }

      // Apply forces
      for (const node of nodes) {
        if (node.id === s.rootId) {
          node.x = centerX;
          node.y = centerY;
          node.vx = 0;
          node.vy = 0;
          continue;
        }
        if (s.drag && s.drag.type === 'node' && s.drag.id === node.id) {
          node.vx = 0;
          node.vy = 0;
          continue;
        }
        const gravity = s.rootId ? 0.0012 + (node.local_depth || 1) * 0.0005 : 0.0018;
        node.vx += (centerX - node.x) * gravity;
        node.vy += (centerY - node.y) * gravity;
        node.x = Math.max(48, Math.min(width - 48, node.x + node.vx));
        node.y = Math.max(48, Math.min(height - 48, node.y + node.vy));
        node.vx *= 0.62;
        node.vy *= 0.62;
      }
    }
  },
};
