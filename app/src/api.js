// API layer. Prefers Tauri `invoke` when running inside the desktop shell,
// otherwise talks to the standalone HTTP server at WSIM_PORT (default 7676).

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const BASE = `http://localhost:${import.meta.env.VITE_WSIM_PORT || 7676}`;

async function invoke(cmd, args) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(cmd, args || {});
}

async function http(path, method, body) {
  const opts = { method: method || "GET", headers: { "Content-Type": "application/json" } };
  if (body !== undefined) opts.body = JSON.stringify(body);
  const res = await fetch(BASE + path, opts);
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  const ct = res.headers.get("content-type") || "";
  return ct.includes("application/json") ? res.json() : res.text();
}

const qs = (obj) =>
  new URLSearchParams(
    Object.entries(obj).filter(([, v]) => v !== undefined && v !== null && v !== "")
  ).toString();

export const api = {
  status: () => (isTauri ? invoke("status") : http("/api/status")),
  listScenarios: () => (isTauri ? invoke("list_scenarios") : http("/api/scenarios")),
  getScenario: (id) => (isTauri ? invoke("get_scenario", { id }) : http(`/api/scenarios/${id}`)),
  createScenario: (p) =>
    isTauri
      ? invoke("create_scenario", { name: p.name, prompt: p.prompt, divergence: p.divergence })
      : http("/api/scenarios", "POST", { name: p.name, prompt: p.prompt, divergence: p.divergence }),
  updateScenario: (id, p) =>
    isTauri
      ? invoke("update_scenario", { id, name: p.name, prompt: p.prompt })
      : http(`/api/scenarios/${id}`, "POST", { name: p.name, prompt: p.prompt }),
  deleteScenario: (id) => (isTauri ? invoke("delete_scenario", { id }) : http(`/api/scenarios/${id}`, "DELETE")),
  branches: (id) => (isTauri ? invoke("branches", { id }) : http(`/api/scenarios/${id}/branches`)),
  simulate: (p) =>
    isTauri
      ? invoke("simulate", {
          scenario_id: p.scenario_id,
          target_date: p.target_date,
          branch_count: p.branch_count,
          force_fallback: p.force_fallback,
        })
      : http("/api/simulate", "POST", p),
  simulateStatus: () => (isTauri ? invoke("simulate_status") : http("/api/simulate/status")),
  world: (scenario, branch, date) =>
    isTauri ? invoke("world", { scenario, branch, date }) : http(`/api/world?${qs({ scenario, branch, date })}`),
  timeline: (scenario, branch) =>
    isTauri ? invoke("timeline", { scenario, branch }) : http(`/api/timeline?${qs({ scenario, branch })}`),
  compare: (scenario, branch) =>
    isTauri ? invoke("compare", { scenario, branch }) : http(`/api/compare?${qs({ scenario, branch })}`),
  refreshNews: () => (isTauri ? invoke("refresh_news") : http("/api/news/refresh", "POST")),
  news: () => (isTauri ? invoke("list_news") : http("/api/news")),
  seedNews: (itemId) =>
    isTauri ? invoke("seed_news", { item_id: itemId }) : http("/api/news/seed", "POST", { item_id: itemId }),
  // Live sim progress: the Tauri shell pushes `sim-progress` events; the
  // standalone server has no push channel, so callers poll `simulateStatus`.
  onSimProgress: (cb) => {
    if (!isTauri) return null;
    return import("@tauri-apps/api/event").then(({ listen }) => listen("sim-progress", (e) => cb(e.payload)));
  },
  // Setup helpers are desktop-only commands.
  engineStatus: () => (isTauri ? invoke("engine_status") : Promise.resolve(null)),
  setupEngine: (force) => (isTauri ? invoke("setup_engine", { force: !!force }) : Promise.resolve(null)),
  downloadModel: (url, filename, force) =>
    isTauri ? invoke("download_model", { url, filename, force: !!force }) : Promise.resolve(null),
  ensureSetup: () => (isTauri ? invoke("ensure_setup") : Promise.resolve(null)),
  setupStatus: () => (isTauri ? invoke("setup_status") : Promise.resolve(null)),
};

export const isDesktop = isTauri;
