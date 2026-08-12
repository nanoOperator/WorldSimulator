# WorldSimulator — Architecture

This document describes how WorldSimulator is built: the data model, the
simulation engine, the validation rules, the model pipeline, and the front‑end.
It is the authoritative reference for contributors.

---

## 1. High‑level design

WorldSimulator is a **local, offline** application. Three layers:

1. **Engine (`crates/engine`)** — a Rust library that owns all simulation
   logic and persistence. Pure, no I/O to the network.
2. **Transport** — either the `worldsim-server` axum HTTP API (port 7676) or
   the Tauri desktop shell (`app/src-tauri`) which embeds the engine directly
   and exposes the same operations as Tauri `invoke` commands.
3. **UI (`app`)** — a React single‑page app (MapLibre + Deck.gl for the 2.5D
   map, plain components for the rest). It talks to the transport via
   `app/src/api.js`, which transparently uses Tauri `invoke` inside the desktop
   shell and `fetch` against the HTTP server otherwise.

The engine is the single source of truth; both transports are thin wrappers.

---

## 2. Time and the timeline

- A `SimDate { year, month, day }` uses **astronomical year numbering**: year 0
  is 1 BCE, year −1 is 2 BCE, etc. `SimDate::default()` is the zero point used
  for "present". `PRESENT_YEAR = 2026`.
- `HISTORY_START = SimDate::from_bce(2_000_000, 1, 1)` — the simulation begins
  with *Homo erectus* and the first controlled use of fire.
- Dates convert to a day count (`days_from_ce`) for fast range queries in
  SQLite (`date_day` index).
- `ERAS` is a 14‑entry table (`crates/engine/src/lib.rs`) used by the UI
  timeline stepper and the seed builder to place era baselines.

### The canonical timeline (immutable)

`data/build_seed.py` produces `data/out/worldsim.db`:

- Downloads Natural Earth `ne_110m_admin_0_countries.geojson` (modern country
  polygons) once, then keeps the simplified geometry offline.
- Emits **58 canonical events**: paleolithic milestones (fire, out‑of‑Africa,
  Neanderthals, cave art, agriculture, writing, …) plus an `EpochBaseline` for
  each era from 3200 BCE to 2020 CE. Each baseline carries the full world
  snapshot (nations, their territories with geometry, and active technologies).
- The modern 2020 baseline is accurate: every country is its own nation with
  real population and polygon.

The canonical timeline is read‑only. Scenarios are layered on top of it.

---

## 3. Data model

### Events (event‑sourced)

`EventPayload` is a Rust `enum` (serde `tag = "kind"`, snake_case). Every event
has `id, date, title, body, payload, source_model, causal_parents, seq`.
Kinds:

| kind | meaning |
|------|---------|
| `epoch_baseline` | full world replacement (canonical eras) |
| `border_change` | a territory changes owner (optionally new geometry) |
| `nation_founded` / `nation_collapsed` | nation lifecycle |
| `war` / `treaty` | conflict and diplomacy |
| `invention` | a technology appears with adoption rate |
| `census` | population + religion/ethnicity/economy/military |
| `migration` | people move between regions |
| `unrest` | riot / guerrilla / rebellion / civil_war / coup |
| `news_seed` | a news item turned into a near‑future scenario seed |
| `narrative` | free‑text historical note |

Each event carries `caused_by: Vec<String>` (event ids) forming a **causal
chain**.

### World snapshot (materialized view)

`WorldSnapshot { date, nations, territories, techs, narrative }`:

- `nations: Nation` with `territories: Vec<String>` (owned ids), population,
  religion/ethnicity percentages, economy/military indices, color.
- `territories: Territory` with `geometry_geojson` and `owner`. Geometry is
  carried by `epoch_baseline` events; `build_snapshot` re‑resolves ownership
  from the nation list so divergence border changes show on the map.
- `to_geojson()` produces a FeatureCollection where each feature is colored by
  its owner — this is what the front‑end renders.

### Storage (`SQLite`, `rusqlite`, bundled)

- `meta`, `canonical_events` (indexed by `date_day`), `scenarios`, `branches`,
  `scenario_events`, `news_sources`, `news_items`.
- Scenario events are validated against the divergence date (see §5).
- `build_snapshot(date, scenario_id, branch_id)` replays canonical + scenario
  events up to `date`.

---

## 4. The simulation engine

`Engine::run_scenario(scenario_id, options, progress)`:

1. Load the scenario (prompt + `divergence_date`).
2. For each branch, run an **adaptive step loop** from the divergence date to
   `target_date` (default 2100), capped by `max_steps`:
   - **Plan.** Build a context: recent world state + relevant canonical
     history retrieved by `ortayli` (RAG) + a system prompt.
   - **Act.** `mustafakemal` proposes the next events as a JSON array
     (`analyze_prompt` routes the prompt; the LLM emits structured events).
   - **Quantify.** `inalcik` fills/checks the numeric fields of those events.
   - **Validate.** `validate` checks structural + world‑context rules and
     `auto_fix`es minor issues (e.g., clamps out‑of‑range percentages).
   - **Self‑check.** If validation can't be satisfied, the step is retried with
     a stricter prompt; repeated failure falls back to a rule‑based step.
   - **Record.** Events are appended with `caused_by` links and the loop
     continues.
3. If models are absent or `force_fallback` is set, the **deterministic
   fallback** (`fallback.rs`) drives the whole branch: it reasons about the
   prompt (keyword routing for wars, conquests, plagues, discoveries) and
   applies plausible border/war/population/invention changes. The app is never
   empty.

`SimulationOptions { branch_count, target_date, force_fallback, max_steps,
temperature, seed }`. Branches run in parallel threads; SQLite WAL keeps writes
safe. A `ProgressCb` reports per‑step progress to the UI.

### Retrieval (ortayli)

`retrieval.rs` embeds canonical passages + curated counterfactuals and answers
queries with cosine‑similarity, giving `mustafakemal` relevant real history
during planning.

### Future prediction (news)

`news.rs` fetches RSS sources (BBC, Guardian, …), scores each by a trust
weight, stores items, and `seed_from_news` turns a headline into a near‑future
scenario seed. This is the "predict what happens next" feature.

---

## 5. Validation & hard rules

- **Divergence hard‑lock.** No scenario event may be dated before the scenario's
  `divergence_date`; attempts return `EngineError::DivergenceLocked`. History is
  immutable.
- **Structural checks.** Required fields, valid enums, non‑negative populations,
  percentages summing to ~100, dates parseable.
- **World‑context checks.** Referenced nations/territories must exist; a
  `border_change` target must be a known territory; a migration's endpoints must
  be known regions.
- **Self‑correction.** Violations trigger `auto_fix` or a retry; persistent
  failure delegates to the fallback planner for that step.

---

## 6. Models & training pipeline (`models/`)

Three qLoRA‑fine‑tuned GGUFs, run locally through **llama.cpp** (the bundled
inference backend; `models/download_models.sh` builds it from source):

| file | base | role |
|------|------|------|
| `mustafakemal-causal-qwen3-8b-q4_k_m.gguf` | Qwen3‑8B | causal simulation |
| `inalcik-data-qwen25-3b-q4_k_m.gguf` | Qwen2.5‑3B | data/statistics |
| `ortayli-embedding-qwen3-0_6b-q4_k_m.gguf` | Qwen3‑Embedding‑0.6B | retrieval |

Pipeline (one command each):

1. `bash models/download_models.sh` — build llama.cpp, download base weights,
   convert + quantize to the filenames above.
2. `python3 models/pipeline/generate_dataset.py` — build the mixed training
   set: curated counterfactuals (historians' what‑ifs) **plus** synthetic
   samples derived from the real canonical timeline.
3. `python3 models/pipeline/train_qlora.py --model <name>` — 4‑bit LoRA
   fine‑tune (needs a CUDA GPU; memory‑friendly).
4. `python3 models/pipeline/merge_lora.py …` — merge adapters into base and
   export the final Q4_K_M GGUFs into `models/`.

The engine loads these from the models directory (configurable via `WSIM_MODELS`
for the server, or the app data dir for Tauri). If they are missing, the
deterministic fallback keeps the app fully functional.

---

## 7. Front‑end (`app/`)

- **WorldMap** — Deck.gl `GeoJsonLayer` over a MapLibre base (blank offline
  style). Territories are extruded by owner population → the "2.5D" look. Click
  selects a territory.
- **Timeline** — a log‑scaled scrubber across 2 million years with era markers.
- **PromptBox / ProgressBox** — enter a divergence; poll `/api/simulate/status`
  (or the Tauri `simulate_status` command) for live progress.
- **ScenarioPanel / BranchTree** — manage scenarios and their parallel branches.
- **StatsPanel** — nations, total population, techs, and a diff vs canonical.
- **api.js** — single transport layer (Tauri `invoke` or `fetch`).

The build is a standard Vite React app; `npm run tauri build` packages it into
the desktop binary with the engine embedded.

---

## 8. API surface (`worldsim-server` / Tauri commands)

`status`, `list_scenarios`, `get_scenario`, `create_scenario`,
`update_scenario`, `delete_scenario`, `branches`, `simulate`,
`simulate_status`, `world`, `timeline`, `compare`, `refresh_news`,
`list_news`, `seed_news`. The server exposes these under `/api/*`; the Tauri
shell exposes them as `invoke` commands with identical semantics.

---

## 9. Testing

- `cargo test` — engine unit tests (date math, event application, validation,
  fallback, retrieval) and an integration test that builds the seed DB, runs a
  scenario with the fallback planner, and reads it back.
- `python3 data/build_seed.py` — rebuilds the canonical database (idempotent).
