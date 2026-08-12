# 🌍 WorldSimulator

**Hold the entire history of the world — from the first controlled fire lit by
*Homo erectus* two million years ago, to the present — and ask "what if?"**

WorldSimulator is a **local, offline desktop application** that simulates
alternate history. You type a divergence ("what if the Nazis won World War
II?", "what if Rome never fell?", "what if Columbus never reached the
Americas?") and the engine reasons about the consequences, branches the
timeline into several plausible outcomes, and renders the changing world on an
interactive 2.5‑dimensional map.

Everything runs **on your machine**. No cloud, no API keys, no telemetry. The
intelligence comes from three small language models that are fine‑tuned for
this exact job and bundled with the app.

---

## Why it exists

Most "alternate history" tools are toys or require sending your prompts to a
remote GPU. WorldSimulator is different:

- It **starts at the deep past** (≈ 2,000,000 BCE — fire, migration out of
  Africa, Neanderthals, the Agricultural Revolution, writing) and walks forward
  through every major era to 2026.
- It **never edits history before your divergence point**. The canonical
  timeline is immutable; scenarios are layered on top of it.
- It is **rigorous about cause and effect**, using a causal chain, a
  self‑checking validator, and retrieval‑augmented planning so the model can
  "remember" real history while inventing the branch.
- It works even **without a GPU**: if the models aren't present, a fast
  deterministic rule‑based simulator produces a believable world so the app is
  never empty.

---

## The three minds

| Name | Base model | Role | Personality |
|------|-----------|------|-------------|
| **mustafakemal** | Qwen3‑8B (qLoRA, Q4_K_M) | Causal simulation | The strategist. Reasons about war, geopolitics, technology, demographics and second‑order consequences. Outputs the structured event stream. |
| **inalcik** | Qwen2.5‑3B (qLoRA, Q4_K_M) | Data & statistics | The accountant. Fills in populations, migrations, economy/military indices and technology adoption so numbers stay internally consistent. |
| **ortayli** | Qwen3‑Embedding‑0.6B (qLoRA, Q4_K_M) | Retrieval | The librarian. Embeds the canonical timeline and your scenario so planning can pull in the relevant real history. |

All three are fine‑tuned with qLoRA and exported to GGUF (Q4_K_M), then run
locally through llama.cpp — the local inference backend that ships with the
app. See `models/` for the full training + export pipeline.

---

## What you can do

- **Diverge.** Type a prompt → a scenario is created at a divergence date and
  simulated forward (default to 2100) across several parallel branches.
- **Explore the map.** Territories are drawn as 2.5D blocks, colored by their
  owning nation and raised by population. Pan, zoom and tilt; click a territory
  to inspect it.
- **Scroll time.** The timeline spans two million years with era markers; click
  one to jump the view to that period.
- **Compare branches.** Each branch is diffed against the canonical world; the
  sidebar lists what changed (borders, nations, techs, populations).
- **Predict the future.** Live RSS news is fetched, scored for trust, and can
  be "seeded" into the engine as a near‑future scenario ("what if this headline
  comes true?").
- **Stay offline.** No network call is required at runtime. News is the only
  optional, user‑triggered online feature.

---

## Architecture at a glance

```
                ┌─────────────────────────────────────────┐
   UI (React)   │  Map (MapLibre+Deck.gl, 2.5D) · Timeline │
        │       │  Prompt · Progress · Scenarios · Branches │
        └───────────────┬───────────────────────────────────┘
                        │  HTTP (localhost:7676)  ·or·  Tauri invoke
                        ▼
        ┌───────────────────────────────────────────────────┐
        │  worldsim-engine (Rust)                             │
        │   • event-sourced WorldSnapshot                     │
        │   • adaptive step loop: plan → mustafakemal →       │
        │     inalcik (stats) → validate → self-check         │
        │   • causal chain (caused_by) · RAG via ortayli      │
        │   • deterministic fallback when models absent       │
        │   • scenario branches + divergence hard-lock        │
        │   • news → future seeds                             │
        └───────┬───────────────────────┬─────────────────────┘
                │                       │
         SQLite (canonical +    llama.cpp (local inference)
         scenario events, news)  mustafakemal · inalcik · ortayli
```

See **ARCHITECTURE.md** for the deep dive (data model, schemas, the simulation
loop, the validation rules, and how to train the models).

---

## Quick start

### Desktop (Tauri)
```bash
cd app
npm install
npm run tauri dev        # builds the UI and launches the desktop window
```
The first launch needs the seed database and (optionally) the models:
```bash
python3 data/build_seed.py                 # builds data/out/worldsim.db
bash models/download_models.sh             # builds llama.cpp + base GGUFs
# optional: python3 models/pipeline/...   # fine-tune mustafakemal/inalcik/ortayli
```

### Headless / web server
```bash
python3 data/build_seed.py
WSIM_DB=data/out/worldsim.db WSIM_MODELS=models \
  cargo run -p worldsim-server
# open http://localhost:7676  (serve app/dist with WSIM_STATIC=app/dist)
```

### Run the engine tests
```bash
cargo test                              # engine unit + integration tests
python3 data/build_seed.py              # rebuild the canonical database
```

---

## Device requirements

| Tier | RAM | Disk | Experience |
|------|-----|------|------------|
| Minimum | 8 GB | ~10 GB | CPU fallback simulation, one branch |
| Recommended | 16 GB | ~12 GB | Full 3‑model simulation, 3 branches |
| Comfortable | 32 GB | ~15 GB | Large branches, live news prediction |

On an 8 GB M1 laptop a single branch with the fallback planner completes in
roughly two minutes; with the local models it is somewhat slower but fully
private.

---

## Project layout

```
WorldSimulator/
├── crates/
│   ├── engine/            # the simulation engine (Rust)
│   └── server/            # axum HTTP API on :7676
├── app/                   # React + Tauri desktop front‑end
│   ├── src/               # UI components (map, timeline, panels)
│   └── src-tauri/         # Tauri shell that embeds the engine
├── models/                # qLoRA training + GGUF export pipeline
├── data/                  # canonical history seed builder
├── README.md
└── ARCHITECTURE.md
```

---

## License

MIT — see `LICENSE`. WorldSimulator is free to use, modify, and redistribute.
