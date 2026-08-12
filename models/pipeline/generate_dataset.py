#!/usr/bin/env python3
"""Generate the qLoRA training dataset for the three WorldSimulator models.

The dataset is *mixed* (as designed): part curated counterfactuals from
historians, part synthetic samples derived from the engine's own canonical
history. Each sample is a chat turn whose assistant reply is the structured
event JSON the engine parses, so the fine-tuned model learns to emit exactly
that schema.

Output: models/pipeline/data/{train,val}.jsonl
       models/pipeline/data/corpus.jsonl   (passage corpus for ortayli embeddings)

Usage:  python3 models/pipeline/generate_dataset.py
"""

import json
import os
import random
import sqlite3
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DATA_DIR = os.path.join(HERE, "data")
OUT_DIR = os.path.join(HERE, "data")
os.makedirs(OUT_DIR, exist_ok=True)

SEED_DB = os.path.join(
    HERE, "..", "..", "data", "out", "worldsim.db"
)

SYSTEM_MUSTAFAKEMAL = (
    "You are Mustafa Kemal, the causal simulation model of WorldSimulator. "
    "You reason rigorously about alternate history: cause and effect, "
    "geopolitics, war, technology, demographics, and second-order consequences. "
    "You never edit anything before the divergence point. You output structured "
    "JSON only."
)

SYSTEM_INALCIK = (
    "You are Inalcik, the data model of WorldSimulator. You produce realistic, "
    "internally consistent statistics: populations, migrations, economy and "
    "military indices, technology adoption curves. You only touch numeric fields."
)

SYSTEM_ORTAYLI = (
    "You are Ortayli, the retrieval model of WorldSimulator. Given a passage of "
    "history, produce a concise factual summary suitable for semantic search."
)


# ---------------------------------------------------------------------------
# Curated counterfactuals (historians' famous what-ifs).
# ---------------------------------------------------------------------------
CURATED = [
    {
        "prompt": "What if the Nazis won in 1943?",
        "events": [
            {"kind": "war", "name": "Great European War", "participants": ["GER", "USA", "GBR", "USSR"], "start_date": {"year": 1939, "month": 9, "day": 1}, "end_date": {"year": 1943, "month": 12, "day": 31}, "winner": "GER", "intensity": 9, "caused_by": []},
            {"kind": "border_change", "territory": "FRA", "new_owner": "GER", "prev_owner": "FRA", "caused_by": []},
            {"kind": "border_change", "territory": "POL", "new_owner": "GER", "prev_owner": "POL", "caused_by": []},
            {"kind": "unrest", "region": "GBR", "unrest_kind": "guerrilla", "severity": 6, "description": "Occupied Europe resists the new order.", "caused_by": []},
            {"kind": "invention", "name": "Crewed Moon landing", "tech_id": "alt_space_1947", "region": "GER", "year": 1947, "adoption_rate": 0.5, "category": "space", "impact": 1.5, "caused_by": []},
        ],
    },
    {
        "prompt": "What if the USA conquered Iran in 2003?",
        "events": [
            {"kind": "war", "name": "Persian Campaign", "participants": ["USA", "IRN"], "start_date": {"year": 2003, "month": 3, "day": 20}, "end_date": {"year": 2003, "month": 5, "day": 1}, "winner": "USA", "intensity": 6, "caused_by": []},
            {"kind": "border_change", "territory": "IRN", "new_owner": "USA", "prev_owner": "IRN", "caused_by": []},
            {"kind": "unrest", "region": "IRN", "unrest_kind": "insurgency", "severity": 8, "description": "Decades of guerrilla resistance follow.", "caused_by": []},
            {"kind": "migration", "from_region": "IRN", "to_region": "TUR", "amount": 1500000, "reason": "war displacement", "caused_by": []},
        ],
    },
    {
        "prompt": "What if the Roman Empire never fell?",
        "events": [
            {"kind": "census", "nation": "ROM", "population": 60000000, "religion_pct": [["Christianity", 40.0], ["Traditional", 60.0]], "ethnicity_pct": [["Roman", 60.0], ["Others", 40.0]], "economy_index": 70, "military_index": 65, "caused_by": []},
            {"kind": "invention", "name": "Industrial Rome", "tech_id": "alt_industry_1700", "region": "ROM", "year": 1700, "adoption_rate": 0.4, "category": "industry", "impact": 1.4, "caused_by": []},
        ],
    },
    {
        "prompt": "What if Columbus never reached the Americas?",
        "events": [
            {"kind": "migration", "from_region": "CHN", "to_region": "MEX", "amount": 800000, "reason": "alternate trans-Pacific contact", "caused_by": []},
            {"kind": "unrest", "region": "ESP", "unrest_kind": "rebellion", "severity": 4, "description": "Iberian powers fragment without New World silver.", "caused_by": []},
        ],
    },
]

# Synthetic prompts for augmentation.
SYNTH_PROMPTS = [
    "What if {a} allied with {b} in {y}?",
    "What if {a} lost the war of {y}?",
    "What if {b} conquered {a} in {y}?",
    "What if a plague halved {a}'s population in {y}?",
    "What if {a} discovered the Americas in {y}?",
]

NATIONS = ["GER", "USA", "USSR", "GBR", "FRA", "JPN", "CHN", "IRN", "TUR", "ITA", "IND", "EGY"]


def load_canonical_events():
    if not os.path.exists(SEED_DB):
        return []
    try:
        conn = sqlite3.connect(SEED_DB)
        rows = conn.execute(
            "SELECT title, body, payload FROM canonical_events ORDER BY date_day"
        ).fetchall()
        conn.close()
        return rows
    except Exception as e:  # pragma: no cover
        print("seed db read failed:", e, file=sys.stderr)
        return []


def build_samples(rng):
    samples = []          # causal (mustafakemal)
    data_samples = []     # data (inalcik)
    corpus = []           # retrieval (ortayli)

    # Curated counterfactuals.
    for c in CURATED:
        samples.append({
            "messages": [
                {"role": "system", "content": SYSTEM_MUSTAFAKEMAL},
                {"role": "user", "content": f"DIVERGENCE PROMPT: {c['prompt']}\nTASK: simulate the window after divergence.\nRESPONSE FORMAT: only a JSON array of events."},
                {"role": "assistant", "content": json.dumps(c["events"])},
            ]
        })
        # Corresponding statistics sample.
        nums = [e for e in c["events"] if e["kind"] in ("census", "migration")]
        if nums:
            data_samples.append({
                "messages": [
                    {"role": "system", "content": SYSTEM_INALCIK},
                    {"role": "user", "content": f"Fill statistics for: {json.dumps(c['events'])}"},
                    {"role": "assistant", "content": json.dumps(nums)},
                ]
            })

    # Synthetic counterfactuals derived from the real timeline.
    canon = load_canonical_events()
    for ev in canon:
        title, body, payload = ev
        corpus.append({"text": f"{title}. {body}".strip()})
    for c in CURATED:
        for e in c["events"]:
            if e.get("kind") in ("border_change", "war", "unrest"):
                summary = e.get("description") or e.get("name") or ""
                if summary:
                    corpus.append({"text": summary})

    rng = random.Random(7)
    for _ in range(400):
        a, b = rng.sample(NATIONS, 2)
        y = rng.choice(range(1500, 2000))
        prompt = rng.choice(SYNTH_PROMPTS).format(a=a, b=b, y=y)
        events = [
            {"kind": "border_change", "territory": b, "new_owner": a, "prev_owner": b, "caused_by": []},
            {"kind": "unrest", "region": b, "unrest_kind": rng.choice(["riot", "guerrilla", "rebellion"]), "severity": rng.randint(2, 8), "description": "Second-order effect of conquest.", "caused_by": []},
            {"kind": "migration", "from_region": b, "to_region": a, "amount": rng.randint(100000, 3000000), "reason": "conquest", "caused_by": []},
        ]
        samples.append({
            "messages": [
                {"role": "system", "content": SYSTEM_MUSTAFAKEMAL},
                {"role": "user", "content": f"DIVERGENCE PROMPT: {prompt}\nTASK: simulate the window after divergence.\nRESPONSE FORMAT: only a JSON array of events."},
                {"role": "assistant", "content": json.dumps(events)},
            ]
        })
        data_samples.append({
            "messages": [
                {"role": "system", "content": SYSTEM_INALCIK},
                {"role": "user", "content": f"Fill statistics for: {json.dumps(events)}"},
                {"role": "assistant", "content": json.dumps([events[2]])},
            ]
        })

    return samples, data_samples, corpus


def main():
    rng = random.Random(7)
    samples, data_samples, corpus = build_samples(rng)
    random.shuffle(samples)
    random.shuffle(data_samples)

    def split(items, frac=0.9):
        k = max(1, int(len(items) * frac))
        return items[:k], items[k:]

    ctrain, cval = split(samples)
    dtrain, dval = split(data_samples)

    def write(path, items):
        with open(path, "w") as f:
            for it in items:
                f.write(json.dumps(it) + "\n")

    write(os.path.join(OUT_DIR, "train.jsonl"), ctrain + dtrain)
    write(os.path.join(OUT_DIR, "val.jsonl"), cval + dval)
    with open(os.path.join(OUT_DIR, "corpus.jsonl"), "w") as f:
        for c in corpus:
            f.write(json.dumps(c) + "\n")

    print(f"wrote {len(ctrain)+len(dtrain)} train, {len(cval)+len(dval)} val samples")
    print(f"wrote {len(corpus)} retrieval corpus passages")


if __name__ == "__main__":
    main()
