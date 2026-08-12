# Model pipeline for WorldSimulator

Three local models power the simulation. All are qLoRA fine-tuned from open
Qwen base weights, exported to GGUF (Q4_K_M) and bundled with the app.

| Role        | Name        | Base model            | GGUF file                                            |
|-------------|-------------|-----------------------|------------------------------------------------------|
| Causal sim  | mustafakemal| Qwen3-8B              | mustafakemal-causal-qwen3-8b-q4_k_m.gguf             |
| Data/stats  | inalcik     | Qwen2.5-3B            | inalcik-data-qwen25-3b-q4_k_m.gguf                   |
| Retrieval   | ortayli     | Qwen3-Embedding-0.6B  | ortayli-embedding-qwen3-0_6b-q4_k_m.gguf             |

## Steps (one command each)

```bash
# 0. prerequisites: rust, python3, pip, git, cmake, node
pip install -r pipeline/requirements.txt

# 1. build llama.cpp (local inference backend) + download base weights,
#    convert + quantize to the filenames above.
bash download_models.sh

# 2. build the mixed training dataset (curated + synthetic from real history).
python3 pipeline/generate_dataset.py

# 3. fine-tune each adapter (needs a CUDA GPU; 4-bit LoRA is memory friendly).
python3 pipeline/train_qlora.py --model mustafakemal
python3 pipeline/train_qlora.py --model inalcik
python3 pipeline/train_qlora.py --model ortayli

# 4. merge adapters into base and export the final GGUFs.
python3 pipeline/merge_lora.py \
  --mustafakemal-adapter adapters/mustafakemal \
  --inalcik-adapter    adapters/inalcik \
  --ortayli-adapter    adapters/ortayli
```

`merge_lora.py` writes the GGUFs directly into `models/`, where `worldsim-engine`
looks for them (models dir is configurable via `WSIM_MODELS`).

The embedding (ortayli) corpus is the curated counterfactual summaries plus the
entire canonical timeline; the contrastive objective teaches it to place
historically related passages near each other, which the engine uses for
retrieval-augmented planning (`crate::retrieval`).
