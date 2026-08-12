# WorldSimulator

WorldSimulator is a desktop application that holds the full written history of the world - from the first Sumerian writing around 3200 BCE to the present day - and lets you diverge it.

You type a change, for example "What if the Nazis won in 1943?", and two local LLMs cooperate to redraw the entire world: borders, inventions, religion, demographics, wars, unrest, and more, from the moment of divergence onward. Nothing before the divergence point is ever touched.

The app also runs future-prediction simulations. It can ingest live news via RSS and simulate where the world is heading, or answer what-if questions about ongoing events, such as "What if the USA conquers Iran?".

## Features

- Full world map, rendered in 2.5D (terrain, mountains, elevation) with real borders and country boundaries for every recorded date from 3200 BCE to the present.
- Timeline scrollers with auto-scroll, play/pause/speed controls, go-to-date, and snap-to-change.
- A prompt box where you describe a change; the simulation begins at your divergence point.
- Multiple outcome branches per scenario. Branches can be explored side by side, kept, or discarded. A tree/graph view shows every fork.
- Scenario management: create, save, edit, merge, compare, and overlay scenarios on one map.
- Live progress box during simulation: streaming year-by-year event feed plus updating statistics and map.
- Future prediction mode: fetches live news (RSS), converts it into a world-state seed, and simulates forward.
- Full causal log: every simulated event records its parent events, so you can trace why something happened.
- Export scenarios as video/timelapse animations and HTML reports.
- Both models run locally; no cloud dependency.

## How it works

WorldSimulator ships with two locally-run, qLoRA fine-tuned models in GGUF format (Q4_K_M):

- 7B-class model (Qwen3-8B base) - causal simulation. Reasons through the knock-on effects of a change across decades, and handles the harder planning tasks.
- 3B-class model (Qwen 2.5-3B base) - data and statistics generation. Produces population figures, economic numbers, migration flows, and other quantitative detail.

Both run through llama.cpp, fully offline.

### Simulation model

The simulation is non-deterministic: each run may explore a different plausible timeline, and users can keep one branch or keep several.

Time is stepped adaptively: years far from changes pass coarsely, and time near the divergence point or major events advances with fine detail.

Consistency is enforced by several layers working together:

1. A rules/constraint engine validates world-state invariants (borders are contiguous, populations are non-negative, totals balance).
2. A validator loop: the LLM generates, code validates, and the LLM retries on failure (with auto-fix for simple violations).
3. An LLM self-check pass: the second model verifies the first model's numbers.
4. The models are instructed to reason about second-order effects such as riots, guerrilla movements, rebellions, economic collapse, and refugee flows - not just maps.

Demographics use all of: per-country aggregates (total population, religion %, ethnicity %), grid-based population density that shifts with borders, a quantified migration model driven by wars and events, and model-predicted extrapolation from historical curves.

Technology uses a hybrid approach: real inventions stay anchored with adoption curves per region, and the models may invent genuinely novel technologies when a diverged world requires them (for example, a Nazi victory might lead to the first crewed Moon landing).

### Faithfulness

- The canonical real timeline is stored as immutable facts.
- Scenarios are confined to divergences: a hard lock prevents any edit to history before the divergence point, and any attempted violation is rejected by code.
- Scenarios are stored as SQLite snapshots layered over the canonical timeline.

## Minimum device requirements

Budget tier (the stated minimum for the 2-minute scenario target):

| Tier | RAM | CPU / GPU | Disk | Notes |
|------|-----|----------|------|-------|
| Budget (minimum) | 8 GB | Apple M1 or 4-core x86 CPU, no GPU required | 10 GB free | 7B model runs on CPU; expected simulation of a full scenario in roughly 2 minutes |
| Mid | 16 GB | Apple M1 Pro / RTX 3060 | 15 GB free | 7B model partially GPU-accelerated; scenarios typically well under 2 minutes |
| High | 32 GB | Apple M2 Pro / RTX 4070+ | 20 GB free | Full GPU offload; fastest simulation |

Expected scenario simulation time on budget tier: approximately 2 minutes per full scenario run. Models (bundled GGUF, Q4_K_M) account for roughly 5-6 GB of the disk requirement.

### Supported platforms

macOS (Apple Silicon and Intel), Windows, and Linux. v1.0 ships release binaries for all three.

## Data

- Historical borders and territories: full timeline from 3200 BCE to the present, assembled from public GIS datasets (Natural Earth and similar) at event-date granularity, covering the GB-scale full written history.
- The history database is stored in SQLite with SpatiaLite for spatial queries.
- History is represented three ways at once: structured events (wars, treaties, inventions, censuses), free-text event entries with dates and tags, and a facts graph of entities and their causal relations.
- Live news is ingested by RSS aggregation with trust scoring (outlet credibility, recency, cross-source dedup), stored, and injected into prediction simulations both as structured state seeds and raw context.

## Project layout

```
WorldSimulator/
├── app/          # Tauri shell + React frontend (MapLibre/Deck.gl WebGL rendering)
├── engine/       # Rust simulation engine (world state, constraints, validator, causal log)
├── models/       # Bundled GGUF files and qLoRA training recipes/scripts
├── data/         # History database builders, GIS source ingestion, seeds
├── news/         # RSS aggregator, trust scoring, news-to-state conversion
└── scripts/      # Build, data-import, and release tooling
```

See ARCHITECTURE.txt for the detailed technical design.

## Development

WorldSimulator is open source (MIT). It is built with a Rust backend (Tauri shell) and a React frontend.

The training-data pipeline uses a mix of synthetic scenario data generated with stronger models, community-contributed what-if scenarios, and curated historical counterfactuals from historians.

## Roadmap

- v0.1: repo scaffold, data pipeline for canonical history, map rendering spike.
- v0.2: single-scenario engine loop (state -> LLM -> validate -> apply), progress box, timeline controls.
- v0.3: branching, comparison/overlay, scenario saving, causal log.
- v0.4: live news ingestion and future-prediction mode.
- v0.5: qLoRA fine-tuned model releases and packaged installers for all three platforms.
- v1.0: full release with binaries, bundled models, full timeline data.

## License

MIT. See LICENSE.
