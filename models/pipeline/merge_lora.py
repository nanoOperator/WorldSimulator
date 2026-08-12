#!/usr/bin/env python3
"""Merge trained qLoRA adapters into the base models and export GGUF (Q4_K_M).

Produces the three files the engine loads at runtime:
  models/mustafakemal-causal-qwen3-8b-q4_k_m.gguf
  models/inalcik-data-qwen25-3b-q4_k_m.gguf
  models/ortayli-embedding-qwen3-0_6b-q4_k_m.gguf

Run `download_models.sh` and `train_qlora.py` first, then this.

Usage:  python3 models/pipeline/merge_lora.py \
          --mustafakemal-adapter models/adapters/mustafakemal \
          --inalcik-adapter    models/adapters/inalcik \
          --ortayli-adapter    models/adapters/ortayli
"""

import argparse
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
MODELS = os.path.join(HERE, "..")

SPECS = {
    "mustafakemal": {
        "base": os.path.join(MODELS, "base", "qwen3-8b"),
        "f16": os.path.join(MODELS, "gguf", "qwen3-8b-f16.gguf"),
        "out": os.path.join(MODELS, "mustafakemal-causal-qwen3-8b-q4_k_m.gguf"),
    },
    "inalcik": {
        "base": os.path.join(MODELS, "base", "qwen25-3b"),
        "f16": os.path.join(MODELS, "gguf", "qwen25-3b-f16.gguf"),
        "out": os.path.join(MODELS, "inalcik-data-qwen25-3b-q4_k_m.gguf"),
    },
    "ortayli": {
        "base": os.path.join(MODELS, "base", "qwen3-embed-0.6b"),
        "f16": os.path.join(MODELS, "gguf", "qwen3-embed-f16.gguf"),
        "out": os.path.join(MODELS, "ortayli-embedding-qwen3-0_6b-q4_k_m.gguf"),
    },
}


def merge_adapter(base, adapter, merged_dir):
    """Merge LoRA into the base using peft and save a full HF model."""
    from peft import PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer
    base_model = AutoModelForCausalLM.from_pretrained(base, torch_dtype="auto", trust_remote_code=True)
    model = PeftModel.from_pretrained(base_model, adapter)
    model = model.merge_and_unload()
    os.makedirs(merged_dir, exist_ok=True)
    model.save_pretrained(merged_dir)
    AutoTokenizer.from_pretrained(base, trust_remote_code=True).save_pretrained(merged_dir)
    return merged_dir


def convert_and_quantize(merged_dir, f16, out):
    llama = os.path.join(MODELS, "llama.cpp")
    conv = os.path.join(llama, "convert_hf_to_gguf.py")
    quant = os.path.join(llama, "build", "bin", "llama-quantize")
    if not os.path.exists(conv) or not os.path.exists(quant):
        print("ERROR: llama.cpp not built. Run models/download_models.sh first.", file=sys.stderr)
        sys.exit(1)
    subprocess.check_call([sys.executable, conv, merged_dir, "--outfile", f16, "--outtype", "f16"])
    subprocess.check_call([quant, f16, out, "Q4_K_M"])
    print(f"wrote {out}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--mustafakemal-adapter")
    ap.add_argument("--inalcik-adapter")
    ap.add_argument("--ortayli-adapter")
    args = ap.parse_args()

    adapters = {
        "mustafakemal": args.mustafakemal_adapter,
        "inalcik": args.inalcik_adapter,
        "ortayli": args.ortayli_adapter,
    }
    for name, adapter in adapters.items():
        if not adapter:
            continue
        spec = SPECS[name]
        merged = os.path.join(MODELS, "merged", name)
        merge_adapter(spec["base"], adapter, merged)
        convert_and_quantize(merged, spec["f16"], spec["out"])


if __name__ == "__main__":
    main()
