//! Local LLM client wrapping llama.cpp.
//!
//! Supports two backends:
//! - `llama-server` over HTTP (recommended; JSON responses with grammar).
//! - `llama-cli` subprocess (simple streaming).
//!
//! If neither binary nor the GGUF weights exist, [`LlamaClient::available`]
//! returns false and the engine falls back to the deterministic simulator.

use crate::models::{ModelSpec, ModelRole};
use crate::{EngineError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A llama-server subprocess owned by the client; killed on drop.
pub(crate) struct ManagedServer {
    child: std::process::Child,
    url: String,
}

impl Drop for ManagedServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct LlamaClient {
    pub models_dir: PathBuf,
    binary_dir: PathBuf,
    server_url: Option<String>,
    server_failed: AtomicBool,
    managed_servers: Mutex<HashMap<String, ManagedServer>>,
    /// Per-model startup lock: only one thread spawns a new server at a time.
    spawn_locks: Mutex<HashMap<String, std::sync::Arc<Mutex<()>>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmResult {
    pub model: String,
    pub text: String,
    pub usage_ms: u64,
}

impl LlamaClient {
    pub fn new(models_dir: impl AsRef<Path>, binary_dir: impl AsRef<Path>) -> Self {
        let binary_dir = binary_dir.as_ref().to_path_buf();
        let server_url = std::env::var("WSIM_LLAMA_SERVER")
            .ok()
            .filter(|s| !s.is_empty());
        LlamaClient {
            models_dir: models_dir.as_ref().to_path_buf(),
            binary_dir,
            server_url,
            server_failed: AtomicBool::new(false),
            managed_servers: Mutex::new(HashMap::new()),
            spawn_locks: Mutex::new(HashMap::new()),
        }
    }

    /// The directory containing llama-cli's shared libraries.
    fn lib_dir(&self) -> PathBuf {
        self.binary_dir.join("llama-b10405")
    }

    /// Directory where the llama.cpp binaries live (used by embed clients too).
    pub fn binary_dir(&self) -> &Path {
        &self.binary_dir
    }

    /// Quick check that the binary can actually start (dylibs present).
    fn binary_runs(&self, binary: &std::path::Path) -> bool {
        let lib_dir = self.lib_dir();
        let mut cmd = Command::new(binary);
        if lib_dir.is_dir() {
            cmd.env("DYLD_LIBRARY_PATH", lib_dir);
        }
        let result = cmd
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(result, Ok(s) if s.success())
    }

    /// Whether at least one backend + model set is usable.
    pub fn available(&self, spec: &ModelSpec) -> bool {
        if self.server_url.is_some() && !self.server_failed.load(Ordering::SeqCst) {
            return true;
        }
        if !crate::models::model_available(&self.models_dir, spec) {
            return false;
        }
        self.find_binary("llama-server").is_some() || self.find_binary("llama-cli").is_some()
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

    /// Get or create a per-model spawn lock (Arc<Mutex<()>>).
    fn spawn_lock_for(&self, model_id: &str) -> Result<std::sync::Arc<Mutex<()>>> {
        let mut locks = self
            .spawn_locks
            .lock()
            .map_err(|_| EngineError::Llm("spawn_locks poisoned".into()))?;
        Ok(locks
            .entry(model_id.to_string())
            .or_insert_with(|| std::sync::Arc::new(Mutex::new(())))
            .clone())
    }

    /// Ensure a llama-server instance is running for the requested model spec.
    ///
    /// The `managed_servers` mutex is held only during quick map operations.
    /// The slow /health poll loop runs with NO locks held so concurrent branch
    /// threads are never deadlocked.  A per-model `spawn_lock` prevents two
    /// threads from racing to spawn a second server for the same model.
    fn ensure_server(&self, spec: &ModelSpec) -> Result<Option<String>> {
        // Fast path: external server configured via env.
        if let Some(url) = &self.server_url {
            if !self.server_failed.load(Ordering::SeqCst) {
                return Ok(Some(url.clone()));
            }
        }

        // Fast path: already have a healthy managed server (brief lock).
        {
            let servers = self
                .managed_servers
                .lock()
                .map_err(|_| EngineError::Llm("mutex poisoned".into()))?;
            if let Some(m) = servers.get(spec.id) {
                let client = reqwest::blocking::Client::new();
                let healthy = client
                    .get(format!("{}/health", m.url))
                    .timeout(Duration::from_millis(600))
                    .send()
                    .map(|r| r.status().is_success())
                    .unwrap_or(false);
                if healthy {
                    return Ok(Some(m.url.clone()));
                }
                // Stale — fall through to respawn.
            }
        }

        // Per-model spawn lock: only one thread runs the slow spawn+poll path.
        // Other threads for the same model will block here (not on managed_servers).
        let spawn_lock = self.spawn_lock_for(spec.id)?;
        let _spawn_guard = spawn_lock
            .lock()
            .map_err(|_| EngineError::Llm("spawn_lock poisoned".into()))?;

        // Re-check after acquiring the spawn lock — a sibling thread may have
        // already started the server while we were waiting.
        {
            let servers = self
                .managed_servers
                .lock()
                .map_err(|_| EngineError::Llm("mutex poisoned".into()))?;
            if let Some(m) = servers.get(spec.id) {
                let client = reqwest::blocking::Client::new();
                let healthy = client
                    .get(format!("{}/health", m.url))
                    .timeout(Duration::from_millis(600))
                    .send()
                    .map(|r| r.status().is_success())
                    .unwrap_or(false);
                if healthy {
                    return Ok(Some(m.url.clone()));
                }
            }
        }

        // Need to spawn a new server.
        let Some(binary) = self.find_binary("llama-server") else {
            return Ok(None);
        };
        let model_path = crate::models::model_path(&self.models_dir, spec);
        if !model_path.is_file() {
            return Err(EngineError::ModelUnavailable(spec.id.into()));
        }

        let port = pick_port()?;
        let url = format!("http://127.0.0.1:{port}");
        let ncpu = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8);
        let ctx_size = match spec.id {
            "mustafakemal" => "4096",
            "inalcik" => "2048",
            _ => "2048",
        };

        let mut cmd = Command::new(&binary);
        if self.lib_dir().is_dir() {
            cmd.env("DYLD_LIBRARY_PATH", self.lib_dir());
        }
        cmd.arg("-m")
            .arg(&model_path)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("-c")
            .arg(ctx_size)
            .arg("-t")
            .arg(ncpu.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| EngineError::Llm(e.to_string()))?;
        log::info!("spawned llama-server for {} on port {port}", spec.id);

        // Insert into map (brief lock), then release before slow health poll.
        {
            let mut servers = self
                .managed_servers
                .lock()
                .map_err(|_| EngineError::Llm("mutex poisoned".into()))?;
            servers.remove(spec.id); // drop any stale entry
            servers.insert(
                spec.id.to_string(),
                ManagedServer { child, url: url.clone() },
            );
        } // <-- managed_servers mutex released here

        // Poll /health with NO locks held — other threads remain unblocked.
        let deadline = Instant::now() + Duration::from_secs(90);
        let client = reqwest::blocking::Client::new();
        loop {
            let ok = client
                .get(format!("{url}/health"))
                .timeout(Duration::from_secs(1))
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if ok {
                log::info!("llama-server for {} ready at {url}", spec.id);
                return Ok(Some(url));
            }
            if Instant::now() >= deadline {
                if let Ok(mut servers) = self.managed_servers.lock() {
                    servers.remove(spec.id);
                }
                return Err(EngineError::Llm(format!(
                    "llama-server for {} did not become healthy in 90s",
                    spec.id
                )));
            }
            std::thread::sleep(Duration::from_millis(400));
        }
    }

    /// Run a completion with the given model. Returns None when unavailable.
    pub fn generate(
        &self,
        spec: &ModelSpec,
        system: &str,
        prompt: &str,
        temp: f64,
        seed: u64,
        max_tokens: usize,
    ) -> Result<Option<LlmResult>> {
        // 1. Try managed llama-server (or external server) over HTTP.
        //    If that fails, surface the error immediately — do NOT fall through
        //    to the CLI path. llama-cli b10405 hangs indefinitely in chat mode
        //    when the GGUF has a chat template, even with --no-conversation.
        match self.ensure_server(spec) {
            Ok(Some(url)) => {
                match self.generate_http(&url, spec, system, prompt, temp, seed, max_tokens) {
                    Ok(r) => return Ok(Some(r)),
                    Err(e) => {
                        // Remove stale server so next call respawns it.
                        if let Ok(mut servers) = self.managed_servers.lock() {
                            servers.remove(spec.id);
                        }
                        return Err(EngineError::Llm(format!(
                            "llama-server HTTP error for {}: {e}",
                            spec.id
                        )));
                    }
                }
            }
            Ok(None) => {
                // No server binary available — try CLI as genuine last resort.
                log::warn!(
                    "no llama-server binary for {}; trying CLI (may hang)",
                    spec.id
                );
                return self.generate_cli(spec, system, prompt, temp, seed, max_tokens);
            }
            Err(e) => {
                return Err(EngineError::Llm(format!(
                    "ensure_server failed for {}: {e}",
                    spec.id
                )));
            }
        }

        #[allow(unreachable_code)]
        Ok(None)
    }

    fn generate_http(
        &self,
        url: &str,
        spec: &ModelSpec,
        system: &str,
        prompt: &str,
        temp: f64,
        seed: u64,
        max_tokens: usize,
    ) -> Result<LlmResult> {
        let full = format!("{system}\n\nUSER:\n{prompt}\nASSISTANT:");
        let client = reqwest::blocking::Client::new();
        let started = std::time::Instant::now();
        let body = client
            .post(format!("{url}/completion"))
            .json(&serde_json::json!({
                "prompt": full,
                "temperature": temp,
                "seed": seed,
                "n_predict": max_tokens,
                "cache_prompt": true,
            }))
            .timeout(std::time::Duration::from_secs(300))
            .send()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        if !body.status().is_success() {
            return Err(EngineError::Http(format!("status {}", body.status())));
        }
        let json: serde_json::Value = body
            .json()
            .map_err(|e| EngineError::Http(e.to_string()))?;
        let text = json
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(LlmResult {
            model: spec.id.to_string(),
            text,
            usage_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn generate_cli(
        &self,
        spec: &ModelSpec,
        system: &str,
        prompt: &str,
        temp: f64,
        seed: u64,
        max_tokens: usize,
    ) -> Result<Option<LlmResult>> {
        let model_path = crate::models::model_path(&self.models_dir, spec);
        if !model_path.is_file() {
            return Ok(None);
        }
        let Some(binary) = self.find_binary("llama-cli") else {
            return Ok(None);
        };

        // Verify the binary is actually runnable (dylibs present, etc.).
        if !self.binary_runs(&binary) {
            return Ok(None);
        }

        let full = format!("{system}\n\nUSER:\n{prompt}\nASSISTANT:");
        let started = std::time::Instant::now();

        // Spawn the child and enforce a wall-clock timeout.
        let mut child = {
            let mut cmd = Command::new(&binary);
            if self.lib_dir().is_dir() {
                cmd.env("DYLD_LIBRARY_PATH", self.lib_dir());
            }
            cmd.arg("-m")
                .arg(&model_path)
                .arg("-p")
                .arg(&full)
                .arg("-n")
                .arg(max_tokens.to_string())
                .arg("--temp")
                .arg(temp.to_string())
                .arg("--seed")
                .arg(seed.to_string())
                .arg("--no-display-prompt")
                .arg("--no-conversation")
                .arg("--simple-io")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| EngineError::Llm(e.to_string()))?
        };

        let timeout = std::time::Duration::from_secs(180);
        let out = match wait_or_kill(&mut child, timeout) {
            Some(res) => res.map_err(|e| EngineError::Llm(e.to_string()))?,
            None => {
                return Err(EngineError::Llm(format!(
                    "llama-cli timed out after {timeout:?}"
                )));
            }
        };

        if !out.status.success() {
            return Err(EngineError::Llm(
                String::from_utf8_lossy(&out.stderr).to_string(),
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(Some(LlmResult {
            model: spec.id.to_string(),
            text,
            usage_ms: started.elapsed().as_millis() as u64,
        }))
    }
}

/// Pick a free ephemeral TCP port on localhost.
pub(crate) fn pick_port() -> Result<u16> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| EngineError::Llm(e.to_string()))?;
    let port = l.local_addr().map_err(|e| EngineError::Llm(e.to_string()))?.port();
    drop(l);
    Ok(port)
}

/// Wait for a child process to finish, or kill it after `timeout` and return None.
pub(crate) fn wait_or_kill(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
) -> Option<std::io::Result<std::process::Output>> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = child.stdout.take();
                let stderr = child.stderr.take();
                use std::io::Read;
                let mut out = Vec::new();
                let mut err = Vec::new();
                if let Some(mut s) = stdout {
                    let _ = s.read_to_end(&mut out);
                }
                if let Some(mut e) = stderr {
                    let _ = e.read_to_end(&mut err);
                }
                return Some(Ok(std::process::Output { status, stdout: out, stderr: err }));
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => return Some(Err(e)),
        }
    }
}

pub fn role_of(spec: &ModelSpec) -> ModelRole {
    spec.role
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires ~/.worldsim/bin and ~/.worldsim/models"]
    fn generate_live_mustafakemal() {
        let home = std::env::var("HOME").expect("HOME");
        let base = std::path::PathBuf::from(&home).join(".worldsim");
        let models = base.join("models");
        let bin = base.join("bin");
        let client = LlamaClient::new(&models, &bin);
        assert!(client.available(&crate::models::MUSTAFAKEMAL));
        let res = client
            .generate(
                &crate::models::MUSTAFAKEMAL,
                "You are an assistant.",
                "Say 'Hello World' in JSON format: {\"greeting\": \"Hello World\"}",
                0.2,
                42,
                64,
            )
            .expect("generate");
        assert!(res.is_some());
        let res = res.unwrap();
        assert!(!res.text.is_empty());
        println!("mustafakemal output (in {}ms): {}", res.usage_ms, res.text);
    }
}
