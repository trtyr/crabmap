/* main.js — Entry point, bootstraps all modules */

CG.main = {
  init() {
    CG.graphInteract.init();
    CG.toolbar.init();
    CG.sidebar.init();
    CG.details.init();

    // Initial status poll
    CG.toolbar.refreshStatus();

    // Polling for status changes
    setInterval(async () => {
      const s = CG.getState();
      const previous = s.status && `${s.status.last_event}:${s.status.last_index_ms}:${s.status.indexing}`;
      await CG.toolbar.refreshStatus();
      const next = s.status && `${s.status.last_event}:${s.status.last_index_ms}:${s.status.indexing}`;
      if (previous && next !== previous && s.status.last_event === 'ready') {
        await CG.toolbar.loadGraph();
      }
    }, 1500);
  },
};

document.addEventListener('DOMContentLoaded', () => {
  CG.main.init();
});
