/* core.js — Microkernel: state store + event bus */

window.CG = (() => {
  const state = {
    status: null,
    graph: null,
    nodesById: new Map(),
    nodePositions: new Map(),
    rootId: null,
    selected: null,
    searchItems: [],
    view: { x: 0, y: 0, scale: 1 },
    drag: null,
    suppressClick: false,
  };

  const listeners = {};
  const modules = {};

  return {
    getState() { return state; },

    setState(patch) {
      Object.assign(state, patch);
      CG.emit('state:change', patch);
    },

    on(event, fn) {
      if (!listeners[event]) listeners[event] = [];
      listeners[event].push(fn);
    },

    emit(event, data) {
      if (!listeners[event]) return;
      for (const fn of listeners[event]) {
        try { fn(data); } catch (e) { console.error(`[CG] event ${event}:`, e); }
      }
    },

    off(event, fn) {
      if (!listeners[event]) return;
      listeners[event] = listeners[event].filter(f => f !== fn);
    },

    register(name, mod) {
      modules[name] = mod;
    },
  };
})();
