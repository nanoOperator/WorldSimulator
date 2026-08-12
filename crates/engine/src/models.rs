//! Model registry: `mustafakemal` (7B-class causal simulation), `inalcik` (3B-class
//! data/statistics). Both are qLoRA fine-tuned GGUF files at Q4_K_M.

use std::path::{Path, PathBuf};

/// What a model is responsible for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ModelRole {
    /// Hard jobs: causal chains, geopolitics, second-order effects.
    Causal,
    /// Bulk data: population, economy, migration, adoption curves.
    Data,
    /// Semantic retrieval: embeddings for history fact lookup (RAG).
    Embedding,
}

/// Static description of one of the two bundled models.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub base_model: &'static str,
    /// Nominal parameter count, used for display.
    pub size_b: f32,
    pub role: ModelRole,
    pub quantization: &'static str,
    /// GGUF filename relative to the models directory.
    pub filename: &'static str,
    /// Recommended context length.
    pub context: usize,
}

/// The two shipped models.
pub const MUSTAFAKEMAL: ModelSpec = ModelSpec {
    id: "mustafakemal",
    name: "Mustafa Kemal",
    base_model: "Qwen3-8B",
    size_b: 8.0,
    role: ModelRole::Causal,
    quantization: "Q4_K_M",
    filename: "mustafakemal-causal-qwen3-8b-q4_k_m.gguf",
    context: 8192,
};

pub const INALCIK: ModelSpec = ModelSpec {
    id: "inalcik",
    name: "Inalcik",
    base_model: "Qwen2.5-3B",
    size_b: 3.0,
    role: ModelRole::Data,
    quantization: "Q4_K_M",
    filename: "inalcik-data-qwen25-3b-q4_k_m.gguf",
    context: 8192,
};

/// Ortayli: Qwen embedding model used for semantic retrieval over the history
/// database. Relevant canonical facts are embedded and retrieved to enrich
/// the LLM context (RAG), which measurably improves output quality.
pub const ORTAYLI: ModelSpec = ModelSpec {
    id: "ortayli",
    name: "Ortayli",
    base_model: "Qwen3-Embedding-0.6B",
    size_b: 0.6,
    role: ModelRole::Embedding,
    quantization: "Q4_K_M",
    filename: "ortayli-embedding-qwen3-0_6b-q4_k_m.gguf",
    context: 32_768,
};

/// Locate a model's GGUF file relative to the models directory.
pub fn model_path(models_dir: &Path, spec: &ModelSpec) -> PathBuf {
    models_dir.join(&spec.filename)
}

/// All three bundled models.
pub fn all_models() -> [ModelSpec; 3] {
    [MUSTAFAKEMAL, INALCIK, ORTAYLI]
}

/// True when a model's GGUF file exists on disk.
pub fn model_available(models_dir: &Path, spec: &ModelSpec) -> bool {
    model_path(models_dir, spec).is_file()
}

/// Route a task to the right model. Hard/planning jobs go to mustafakemal; bulk
/// numeric jobs go to inalcik.
pub fn route(task: &str) -> ModelSpec {
    let t = task.to_lowercase();
    const HARD: [&str; 14] = [
        "plan", "causal", "war", "invasion", "treaty", "revolution", "collapse",
        "nazi", "conquer", "narrative", "second-order", "riot", "guerrilla",
        "rebellion",
    ];
    if HARD.iter().any(|k| t.contains(k)) {
        MUSTAFAKEMAL
    } else {
        INALCIK
    }
}
