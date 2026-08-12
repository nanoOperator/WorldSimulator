#!/usr/bin/env bash
# WorldSimulator model acquisition + GGUF build.
#
# Produces the three bundled models the engine expects:
#   mustafakemal-causal-qwen3-8b-q4_k_m.gguf   (Qwen3-8B,  causal simulation)
#   inalcik-data-qwen25-3b-q4_k_m.gguf         (Qwen2.5-3B, data/statistics)
#   ortayli-embedding-qwen3-0_6b-q4_k_m.gguf   (Qwen3-Embedding-0.6B)
#
# Steps:
#   1. Install llama.cpp (built locally into ./llama.cpp/build).
#   2. Download base weights (HF safetensors) for each model.
#   3. Convert to GGUF and quantize to Q4_K_M with the filenames above.
#   4. Optionally merge a trained qLoRA adapter with `merge_lora.py`.
#
# Requires: git, cmake, python3, pip, ~30 GB free, and (for step 3) internet.
# Usage:  bash models/download_models.sh            # base GGUFs only
#         bash models/download_models.sh --with-lora # also merge trained adapters

set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE"
mkdir -p ./base ./gguf ./llama.cpp

# ---------------------------------------------------------------------------
# 1. Build llama.cpp (the "special" local inference backend).
# ---------------------------------------------------------------------------
if [ ! -x ./llama.cpp/build/bin/llama-quantize ]; then
  echo "==> Building llama.cpp"
  if [ ! -d ./llama.cpp/.git ]; then
    git clone --depth 1 https://github.com/ggml-org/llama.cpp.git ./llama.cpp
  fi
  cmake -S ./llama.cpp -B ./llama.cpp/build -DGGML_CUDA=OFF -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=ON
  cmake --build ./llama.cpp/build --config Release -j"$(sysctl -n hw.ncpu 2>/dev/null || echo 4)"
fi
LLAMA_BIN="$HERE/llama.cpp/build/bin"

# ---------------------------------------------------------------------------
# 2. Download base weights as HF safetensors.
# ---------------------------------------------------------------------------
pip install -q --disable-pip-version-check "huggingface_hub[cli]" 2>/dev/null || true

download_safetensors () {  # repo  subpath  outdir
  echo "==> Downloading $1/$2"
  python3 - "$1" "$2" "$3" <<'PY'
import sys, os
from huggingface_hub import snapshot_download, hf_hub_download
repo, sub, out = sys.argv[1], sys.argv[2], sys.argv[3]
os.makedirs(out, exist_ok=True)
hf_hub_download(repo_id=repo, filename=sub, local_dir=out, local_dir_use_symlinks=False)
PY
}

# Base model repos (HF).
download_safetensors "Qwen/Qwen3-8B"      "model.safetensors"       "./base/qwen3-8b"
download_safetensors "Qwen/Qwen2.5-3B"   "model.safetensors"       "./base/qwen25-3b"
download_safetensors "Qwen/Qwen3-Embedding-0.6B" "model.safetensors" "./base/qwen3-embed-0.6b"

# ---------------------------------------------------------------------------
# 3. Convert -> quantize to Q4_K_M, with the exact filenames the engine uses.
# ---------------------------------------------------------------------------
quantize () {  # f16_gguf  out_gguf
  "$LLAMA_BIN/llama-quantize" "$1" "$2" Q4_K_M
}

for spec in \
  "base/qwen3-8b:/qwen3-8b-f16.gguf:mustafakemal-causal-qwen3-8b-q4_k_m.gguf" \
  "base/qwen25-3b:/qwen25-3b-f16.gguf:inalcik-data-qwen25-3b-q4_k_m.gguf" \
  "base/qwen3-embed-0.6b:/qwen3-embed-f16.gguf:ortayli-embedding-qwen3-0_6b-q4_k_m.gguf" ; do
  base="${spec%%:*}"; rest="${spec#*:}"; f16="${rest%%:*}"; out="${rest##*:}"
  echo "==> Converting $base -> $f16"
  python3 ./llama.cpp/convert_hf_to_gguf.py "$base" --outfile "./gguf/$f16" --outtype f16
  echo "==> Quantizing ./gguf/$f16 -> ./$out (Q4_K_M)"
  quantize "./gguf/$f16" "./$out"
done

# ---------------------------------------------------------------------------
# 4. Optionally merge a trained qLoRA adapter (run train_qlora.py first).
# ---------------------------------------------------------------------------
if [ "${1:-}" = "--with-lora" ]; then
  echo "==> Merging qLoRA adapters"
  python3 ./pipeline/merge_lora.py \
    --mustafakemal-adapter ./adapters/mustafakemal \
    --inalcik-adapter    ./adapters/inalcik \
    --ortayli-adapter    ./adapters/ortayli
fi

echo "==> Done. Models:"
ls -lh ./*.gguf
