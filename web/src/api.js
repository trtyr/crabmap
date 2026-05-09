/* api.js — HTTP client for /api/* endpoints */

CG.api = {
  async request(path, options) {
    const response = await fetch(path, options);
    if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
    return response.json();
  },

  status() {
    return CG.api.request('/api/status');
  },

  graph() {
    return CG.api.request('/api/graph');
  },

  search(q, limit) {
    return CG.api.request(`/api/search?q=${encodeURIComponent(q)}&limit=${limit}`);
  },

  reindex() {
    return CG.api.request('/api/reindex', { method: 'POST' });
  },
};
