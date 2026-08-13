//! Semantic retrieval (RAG) powered by `ortayli`, the bundled Qwen embedding
//! model. Relevant historical facts are embedded once and retrieved per query
//! by cosine similarity, enriching the context handed to mustafakemal/inalcik.

use crate::llm::LlamaClient;
use crate::models::ORTAYLI;
use crate::{EngineError, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// An embedding produced by ortayli.
pub type Embedding = Vec<f32>;

pub struct EmbedClient {
    models_dir: PathBuf,
    server_url: Option<String>,
}

impl EmbedClient {
    pub fn new(models_dir: impl AsRef<Path>) -> Self {
        let server_url = std::env::var("WSIM_LLAMA_SERVER").ok().filter(|s| !s.is_empty());
        EmbedClient {
            models_dir: models_dir.as_ref().to_path_buf(),
            server_url,
        }
    }

    pub fn available(&self) -> bool {
        self.server_url.is_some() || crate::models::model_available(&self.models_dir, &ORTAYLI)
    }

    /// Embed a single text.
    pub fn embed(&self, text: &str) -> Result<Embedding> {
        self.embed_batch(&[text.to_string()])?
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::Llm("empty embedding result".into()))
    }

    /// Embed a batch of texts. Uses llama-server /embedding when configured,
    /// otherwise shells out to `llama-embedding`.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Embedding>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(url) = &self.server_url {
            let mut out = Vec::with_capacity(texts.len());
            for t in texts {
                out.push(self.embed_http(url, t)?);
            }
            return Ok(out);
        }
        self.embed_cli(texts)
    }

    fn embed_http(&self, url: &str, text: &str) -> Result<Embedding> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(format!("{url}/embedding"))
            .json(&serde_json::json!({ "content": text }))
            .timeout(Duration::from_secs(120))
            .send()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(EngineError::Http(format!("status {}", resp.status())));
        }
        let json: serde_json::Value = resp
            .json()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        let arr = json
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| EngineError::Llm("no embedding in response".into()))?;
        let mut v = Vec::with_capacity(arr.len());
        for x in arr {
            v.push(x.as_f64().unwrap_or(0.0) as f32);
        }
        Ok(v)
    }

    fn embed_cli(&self, texts: &[String]) -> Result<Vec<Embedding>> {
        let model_path = crate::models::model_path(&self.models_dir, &ORTAYLI);
        if !model_path.is_file() {
            return Err(EngineError::ModelUnavailable(ORTAYLI.id.into()));
        }
        // Prefer `llama-embedding`; fall back to `llama-cli --embedding`.
        let binary = ["llama-embedding", "llama-cli"]
            .iter()
            .find_map(|name| find_in_path(name).or_else(|| find_in_path(&format!("{name}.exe"))));
        let Some(binary) = binary else {
            return Err(EngineError::ModelUnavailable("llama-embedding not found".into()));
        };
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            let res = std::process::Command::new(&binary)
                .arg("-m")
                .arg(&model_path)
                .arg("-p")
                .arg(t)
                .arg("--embedding")
                .arg("--no-display-prompt")
                .output()
                .map_err(|e| EngineError::Llm(e.to_string()))?;
            let text = String::from_utf8_lossy(&res.stdout);
            let mut nums = Vec::new();
            for tok in text.split([',', ' ', '\n', '[']).filter(|s| !s.trim().is_empty()) {
                let s = tok.trim().trim_end_matches([']', ']']).to_string();
                if let Ok(n) = s.parse::<f32>() {
                    nums.push(n);
                }
            }
            if nums.is_empty() {
                return Err(EngineError::Llm("no embeddings in llama output".into()));
            }
            out.push(nums);
        }
        Ok(out)
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// An in-memory index of embedded documents for cosine-similarity search.
pub struct EmbeddingIndex {
    docs: Vec<String>,
    embs: Vec<Embedding>,
}

impl Default for EmbeddingIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingIndex {
    pub fn new() -> Self {
        EmbeddingIndex { docs: Vec::new(), embs: Vec::new() }
    }

    /// Embed and add documents. Skips empty/duplicate docs.
    pub fn add(&mut self, client: &EmbedClient, docs: &[String]) -> Result<()> {
        let mut fresh = Vec::new();
        for d in docs {
            let d = d.trim();
            if d.is_empty() || self.docs.iter().any(|x| x == d) {
                continue;
            }
            fresh.push(d.to_string());
        }
        if fresh.is_empty() {
            return Ok(());
        }
        let embs = client.embed_batch(&fresh)?;
        self.docs.extend(fresh);
        self.embs.extend(embs);
        Ok(())
    }

    /// Cosine similarity search. Returns (score, doc) sorted descending.
    pub fn query(&self, client: &EmbedClient, query: &str, k: usize) -> Result<Vec<(f32, String)>> {
        if self.docs.is_empty() {
            return Ok(Vec::new());
        }
        let q = client.embed(query)?;
        let mut scored: Vec<(f32, String)> = self
            .embs
            .iter()
            .zip(self.docs.iter())
            .map(|(e, d)| (cosine(&q, e), d.clone()))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        Ok(scored)
    }
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut na = 0.0f64;
    let mut nb = 0.0f64;
    for i in 0..n {
        dot += a[i] as f64 * b[i] as f64;
        na += a[i] as f64 * a[i] as f64;
        nb += b[i] as f64 * b[i] as f64;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        (dot / (na.sqrt() * nb.sqrt())) as f32
    }
}

/// Convenience: build an index from an iterable of documents.
pub fn build_index(client: &EmbedClient, docs: &[String]) -> Result<EmbeddingIndex> {
    let mut idx = EmbeddingIndex::new();
    idx.add(client, docs)?;
    Ok(idx)
}

/// Keep LlamaClient reachable for callers that want to reuse its server config.
#[allow(dead_code)]
fn _unused(_: &LlamaClient) {}
