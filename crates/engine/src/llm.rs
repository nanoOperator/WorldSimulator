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
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct LlamaClient {
    pub models_dir: PathBuf,
    binary_dir: PathBuf,
    server_url: Option<String>,
    server_failed: AtomicBool,
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
        self.find_binary("llama-cli").is_some() || self.find_binary("llama-server").is_some()
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
        if let Some(url) = &self.server_url {
            match self.generate_http(url, spec, system, prompt, temp, seed, max_tokens) {
                Ok(r) => return Ok(Some(r)),
                Err(e) => {
                    self.server_failed.store(true, Ordering::SeqCst);
                    log::warn!("llama-server failed, falling back to CLI: {e}");
                }
            }
        }
        self.generate_cli(spec, system, prompt, temp, seed, max_tokens)
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

        // Spawn the child and enforce a wall-clock timeout. If it hangs
        // (e.g. model too large, OOM, or incompatible binary) we kill it and
        // return Err so run_branch can fall back to deterministic mode.
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
