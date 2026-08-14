//! Semantic retrieval (RAG) powered by `ortayli`, the bundled Qwen embedding
//! model. Relevant historical facts are embedded once and retrieved per query
//! by cosine similarity, enriching the context handed to mustafakemal/inalcik.

use crate::llm::LlamaClient;
use crate::models::ORTAYLI;
use crate::{EngineError, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// An embedding produced by ortayli.
pub type Embedding = Vec<f32>;

pub struct EmbedClient {
    models_dir: PathBuf,
    binary_dir: PathBuf,
    server_url: Option<String>,
    /// A llama-server we spawned ourselves to serve /embedding.
    managed: Option<ManagedServer>,
}

/// A llama-server subprocess owned by the client; killed on drop.
struct ManagedServer {
    child: std::process::Child,
    url: String,
}

impl Drop for ManagedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl EmbedClient {
    pub fn new(models_dir: impl AsRef<Path>, binary_dir: impl AsRef<Path>) -> Self {
        let server_url = std::env::var("WSIM_LLAMA_SERVER").ok().filter(|s| !s.is_empty());
        EmbedClient {
            models_dir: models_dir.as_ref().to_path_buf(),
            binary_dir: binary_dir.as_ref().to_path_buf(),
            server_url,
            managed: None,
        }
    }

    fn lib_dir(&self) -> PathBuf {
        self.binary_dir.join("llama-b10405")
    }

    fn find_binary(&self, name: &str) -> Option<PathBuf> {
        let direct = self.binary_dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        let direct_exe = self.binary_dir.join(format!("{name}.exe"));
        if direct_exe.is_file() {
            return Some(direct_exe);
        }
        // Fall back to PATH lookup.
        let path_var = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path_var) {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    }

    pub fn available(&self) -> bool {
        if self.server_url.is_some() {
            return true;
        }
        if !crate::models::model_available(&self.models_dir, &ORTAYLI) {
            return false;
        }
        self.find_binary("llama-server").is_some()
            || self.find_binary("llama-embedding").is_some()
            || self.find_binary("llama-cli").is_some()
    }

    /// Spawn a local llama-server on an ephemeral port serving /embedding for
    /// ortayli. Returns the base URL once the server reports healthy.
    fn ensure_server(&mut self) -> Result<Option<String>> {
        if let Some(url) = &self.server_url {
            return Ok(Some(url.clone()));
        }
        if let Some(m) = &self.managed {
            return Ok(Some(m.url.clone()));
        }
        let Some(binary) = self.find_binary("llama-server") else {
            return Ok(None);
        };
        let model_path = crate::models::model_path(&self.models_dir, &ORTAYLI);
        if !model_path.is_file() {
            return Err(EngineError::ModelUnavailable(ORTAYLI.id.into()));
        }
        let port = pick_port()?;
        let url = format!("http://127.0.0.1:{port}");
        let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let mut cmd = std::process::Command::new(&binary);
        if self.lib_dir().is_dir() {
            cmd.env("DYLD_LIBRARY_PATH", self.lib_dir());
        }
        cmd.arg("-m")
            .arg(&model_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--embedding")
            .arg("-t")
            .arg(ncpu.to_string())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let child = cmd.spawn().map_err(|e| EngineError::Llm(e.to_string()))?;
        self.managed = Some(ManagedServer { child, url: url.clone() });

        let deadline = Instant::now() + Duration::from_secs(90);
        let client = reqwest::blocking::Client::new();
        loop {
            let ok = client
                .get(format!("{url}/health"))
                .timeout(Duration::from_secs(2))
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if ok {
                return Ok(Some(url));
            }
            if Instant::now() >= deadline {
                return Err(EngineError::Llm(
                    "llama-server did not become healthy in time".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Embed a single text.
    pub fn embed(&mut self, text: &str) -> Result<Embedding> {
        self.embed_batch(&[text.to_string()])?
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::Llm("empty embedding result".into()))
    }

    /// Embed a batch of texts. Uses llama-server /embedding when available
    /// (external or self-spawned), otherwise shells out to `llama-embedding`.
    pub fn embed_batch(&mut self, texts: &[String]) -> Result<Vec<Embedding>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if let Some(url) = self.ensure_server()? {
            return self.embed_batch_http(&url, texts);
        }
        self.embed_cli(texts)
    }

    fn embed_batch_http(&self, url: &str, texts: &[String]) -> Result<Vec<Embedding>> {
        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(format!("{url}/embedding"))
            .json(&serde_json::json!({ "input": texts }))
            .timeout(Duration::from_secs(180))
            .send()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(EngineError::Http(format!("status {}", resp.status())));
        }
        let json: serde_json::Value = resp
            .json()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        let mut out = Vec::with_capacity(texts.len());
        // b10405+: JSON array of {"index":n, "embedding":[[...]]}.
        if let Some(items) = json.as_array() {
            for item in items {
                let emb = item
                    .get("embedding")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| EngineError::Llm("no embedding in response".into()))?;
                // "embedding" is a batch-of-one list of lists (b10405+), or a
                // flat vector on older servers. Take the first vector either way.
                let vec = emb.iter().find_map(|v| v.as_array()).unwrap_or(emb);
                out.push(floats(vec));
            }
        } else if let Some(emb) = json.get("embedding").and_then(|v| v.as_array()) {
            // Older llama-server single response: {"embedding": [...]}.
            out.push(floats(emb));
        } else {
            return Err(EngineError::Llm("unrecognized embedding response".into()));
        }
        Ok(out)
    }

    fn embed_cli(&self, texts: &[String]) -> Result<Vec<Embedding>> {
        let model_path = crate::models::model_path(&self.models_dir, &ORTAYLI);
        if !model_path.is_file() {
            return Err(EngineError::ModelUnavailable(ORTAYLI.id.into()));
        }
        // Prefer `llama-embedding`; fall back to `llama-cli --embedding`.
        let binary = ["llama-embedding", "llama-cli"]
            .iter()
            .find_map(|name| self.find_binary(name).or_else(|| self.find_binary(&format!("{name}.exe"))));
        let Some(binary) = binary else {
            return Err(EngineError::ModelUnavailable("llama-embedding not found".into()));
        };
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            let mut cmd = std::process::Command::new(&binary);
            if self.lib_dir().is_dir() {
                cmd.env("DYLD_LIBRARY_PATH", self.lib_dir());
            }
            let mut child = cmd
                .arg("-m")
                .arg(&model_path)
                .arg("-p")
                .arg(t)
                .arg("--embedding")
                .arg("--no-display-prompt")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|e| EngineError::Llm(e.to_string()))?;
            let timeout = std::time::Duration::from_secs(120);
            let Some(res) = crate::llm::wait_or_kill(&mut child, timeout) else {
                return Err(EngineError::Llm(format!("llama-embedding timed out after {timeout:?}")));
            };
            let res = res.map_err(|e| EngineError::Llm(e.to_string()))?;
            if !res.status.success() {
                return Err(EngineError::Llm(
                    String::from_utf8_lossy(&res.stderr).trim().to_string(),
                ));
            }
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
    pub fn add(&mut self, client: &mut EmbedClient, docs: &[String]) -> Result<()> {
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
    pub fn query(&mut self, client: &mut EmbedClient, query: &str, k: usize) -> Result<Vec<(f32, String)>> {
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

/// Convert a JSON array of numbers into an embedding vector.
fn floats(arr: &[serde_json::Value]) -> Embedding {
    arr.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect()
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
pub fn build_index(client: &mut EmbedClient, docs: &[String]) -> Result<EmbeddingIndex> {
    let mut idx = EmbeddingIndex::new();
    idx.add(client, docs)?;
    Ok(idx)
}

/// Pick a free ephemeral TCP port on localhost for a spawned llama-server.
fn pick_port() -> Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| EngineError::Llm(e.to_string()))?;
    let port = l.local_addr().map_err(|e| EngineError::Llm(e.to_string()))?.port();
    drop(l);
    Ok(port)
}

/// Keep LlamaClient reachable for callers that want to reuse its server config.
#[allow(dead_code)]
fn _unused(_: &LlamaClient) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// Live end-to-end check against the real binary + ortayli GGUF. Skips
    /// when the model has not been downloaded (e.g. CI).
    #[test]
    #[ignore = "requires ~/.worldsim/bin and ~/.worldsim/models"]
    fn embed_batch_live() {
        let home = std::env::var("HOME").expect("HOME");
        let base = std::path::PathBuf::from(&home).join(".worldsim");
        let models = base.join("models");
        let bin = base.join("bin");
        if !models.join(ORTAYLI.filename).is_file() {
            eprintln!("skipping: ortayli GGUF not present");
            return;
        }
        let mut client = EmbedClient::new(&models, &bin);
        assert!(client.available());
        let texts = vec![
            "The Roman Empire fell in 476 CE.".to_string(),
            "The Mongols sacked Baghdad in 1258.".to_string(),
        ];
        let embs = client.embed_batch(&texts).expect("embed");
        assert_eq!(embs.len(), 2);
        assert_eq!(embs[0].len(), 1024);
        // Same text twice must be near-identical.
        let a = client.embed(&texts[0]).expect("re-embed");
        let sim = cosine(&embs[0], &a);
        assert!(sim > 0.99, "self-similarity {sim} too low");
        // Different topics should be clearly less similar.
        let sim2 = cosine(&embs[0], &embs[1]);
        assert!(sim2 < 0.99, "cross-similarity {sim2} unexpectedly high");
    }
}
