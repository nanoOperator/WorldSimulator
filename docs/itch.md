# WorldSimulator

**Hold the history of the world — from the first fire to the present — and ask "what if?"**

WorldSimulator is a local, offline alternate-history simulator. Type a divergence
("what if the Nazis won World War II?", "what if Rome never fell?") and the engine
reasons about the consequences, branches the timeline into several plausible
outcomes, and renders the changing world on an interactive 2.5‑dimensional map.

Everything runs on your machine. No cloud, no API keys, no telemetry. The
intelligence comes from three small language models that are fine‑tuned for this
exact job and bundled with the app.

## Features
- **Diverge** — seed a scenario at any date and simulate forward to 2100 across
  several parallel branches.
- **Explore the map** — territories are 2.5D blocks, colored by owner and raised
  by population. Pan, zoom, tilt, click.
- **Scroll time** — a timeline spanning two million years with era markers.
- **Compare branches** — each branch is diffed against the canonical world.
- **Predict the future** — live RSS news is fetched, scored for trust, and can be
  seeded into a near‑future scenario.
- **Stay offline** — no network call is required at runtime.

## The three minds
- **mustafakemal** — Qwen3‑8B (qLoRA, Q4_K_M) — causal simulation, the strategist.
- **inalcik** — Qwen2.5‑3B (qLoRA, Q4_K_M) — data & statistics, the accountant.
- **ortayli** — Qwen3‑Embedding‑0.6B (qLoRA, Q4_K_M) — retrieval, the librarian.

## Requirements
8 GB RAM, an Apple M1 / Intel / AMD processor, ~10 GB storage. MIT licensed.
