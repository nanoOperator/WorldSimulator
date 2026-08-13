// API layer. Prefers Tauri `invoke` when running inside the desktop shell,
// otherwise talks to the standalone HTTP server at WSIM_PORT (default 7676).

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const BASE = `http://localhost:${import.meta.env.VITE_WSIM_PORT || 7676}`;

async function invokeOrFetch(cmd, args, path, method = "GET", body) {
  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke(cmd, args || {});
  }
  const opts = { method, headers: { "Content-Type": "application/json" } };
  if (body !== undefined) opts.body = JSON.stringify(body);
  const res = await fetch(BASE + path, opts);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const ct = res.headers.get("content-type") || "";
  return ct.includes("application/json") ? res.json() : res.text();
}

export const api = {
  status: () => invokeOrFetch("status", {}, "/api/status"),
  listScenarios: () => invokeOrFetch("list_scenarios", {}, "/api/scenarios"),
  getScenario: (id) => invokeOrFetch("get_scenario", { id }, `/api/scenarios/${id}`),
  createScenario: (payload) =>
    invokeOrFetch("create_scenario", { payload }, "/api/scenarios", "POST", payload),
  updateScenario: (id, payload) =>
    invokeOrFetch("update_scenario", { id, payload }, `/api/scenarios/${id}`, "POST", payload),
  deleteScenario: (id) => invokeOrFetch("delete_scenario", { id }, `/api/scenarios/${id}`, "DELETE"),
  branches: (id) => invokeOrFetch("branches", { id }, `/api/scenarios/${id}/branches`),
  simulate: (payload) => invokeOrFetch("simulate", { payload }, "/api/simulate", "POST", payload),
  simulateStatus: () => invokeOrFetch("simulate_status", {}, "/api/simulate/status"),
  world: (id, branch) =>
    invokeOrFetch("world", { scenario: id, branch: branch || "" }, `/api/world?scenario=${id || ""}&branch=${branch || ""}`),
  timeline: (id, branch, fromDay, toDay) =>
    invokeOrFetch(
      "timeline",
      { scenario: id, branch: branch || "", fromDay: fromDay || 0, toDay: toDay || 0 },
      `/api/timeline?scenario=${id || ""}&branch=${branch || ""}`
    ),
  compare: (id, branch) =>
    invokeOrFetch("compare", { scenario: id, branch: branch || "" }, `/api/compare?scenario=${id}&branch=${branch || ""}`),
  simulateStatus: () => invokeOrFetch("simulate_status", {}, "/api/simulate/status"),
  refreshNews: () => invokeOrFetch("refresh_news", {}, "/api/news/refresh", "POST"),
  news: () => invokeOrFetch("list_news", {}, "/api/news"),
  seedNews: (itemId) => invokeOrFetch("seed_news", { itemId }, "/api/news/seed", "POST", { itemId }),
  engineStatus: () => invokeOrFetch("engine_status", {}),
  setupEngine: (force) => invokeOrFetch("setup_engine", { force: !!force }),
  downloadModel: (url, filename, force) =>
    invokeOrFetch("download_model", { url, filename, force: !!force }),
};

export const isDesktop = isTauri;
