use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use worldsim_engine::{date_from_iso, Engine, SimDate, SimProgress};

// Pinned llama.cpp release used for the one-click engine download.
const LLAMA_TAG: &str = "b10405";

/// Resolve the (asset suffix, is_zip) for the current OS/architecture.
fn llama_asset() -> Option<(&'static str, bool)> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some(("win-cpu-x64.zip", true)),
        ("windows", "aarch64") => Some(("win-cpu-arm64.zip", true)),
        ("macos", "aarch64") => Some(("macos-arm64.tar.gz", false)),
        ("macos", "x86_64") => Some(("macos-x64.tar.gz", false)),
        ("linux", "x86_64") => Some(("ubuntu-x64.tar.gz", false)),
        ("linux", "aarch64") => Some(("ubuntu-arm64.tar.gz", false)),
        _ => None,
    }
}

fn llama_url() -> Option<String> {
    let (suffix, _) = llama_asset()?;
    Some(format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_TAG}/llama-{LLAMA_TAG}-bin-{suffix}"
    ))
}

fn binary_present(dir: &PathBuf, name: &str) -> bool {
    dir.join(name).is_file() || dir.join(format!("{name}.exe")).is_file()
}

/// Preset GGUF downloads. These are verified anonymously-downloadable files
/// (HF "resolve" endpoints) saved under the engine's expected filenames so the
/// simulation can use the local LLM out of the box.
pub fn model_presets() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "id": "mustafakemal",
            "name": "Mustafa Kemal (Qwen2.5-7B)",
            "filename": "mustafakemal-causal-qwen3-8b-q4_k_m.gguf",
            "url": "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
        }),
        serde_json::json!({
            "id": "inalcik",
            "name": "Inalcik (Qwen2.5-3B)",
            "filename": "inalcik-data-qwen25-3b-q4_k_m.gguf",
            "url": "https://huggingface.co/Qwen/Qwen2.5-3B-Instruct-GGUF/resolve/main/qwen2.5-3b-instruct-q4_k_m.gguf"
        }),
        serde_json::json!({
            "id": "ortayli",
            "name": "Ortayli (Qwen3-Embedding-0.6B)",
            "filename": "ortayli-embedding-qwen3-0_6b-q4_k_m.gguf",
            "url": "https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF/resolve/main/Qwen3-Embedding-0.6B-Q8_0.gguf"
        }),
    ]
}

fn preset_for(id: &str) -> Option<(String, String)> {
    model_presets().into_iter().find(|v| v["id"] == id).map(|v| {
        (
            v["url"].as_str().unwrap_or_default().to_string(),
            v["filename"].as_str().unwrap_or_default().to_string(),
        )
    })
}

fn set_status(s: &Arc<Mutex<SetupStatus>>, stage: &str, msg: &str, pct: f64) {
    let mut st = s.lock().unwrap();
    st.stage = stage.into();
    st.message = msg.into();
    st.percent = pct;
}

async fn stream_download(
    app: &AppHandle,
    url: &str,
    dest: &PathBuf,
    event: &str,
    status: Option<&Arc<Mutex<SetupStatus>>>,
    span: (f64, f64),
) -> Result<(), String> {
    use futures_util::StreamExt;
    let client = reqwest::Client::new();
    let resp = client.get(url).send().await.map_err(|e| format!("request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = std::fs::File::create(dest).map_err(|e| format!("create file: {e}"))?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("stream error: {e}"))?;
        file.write_all(&chunk).map_err(|e| format!("write: {e}"))?;
        downloaded += chunk.len() as u64;
        let percent = if total > 0 { downloaded as f64 / total as f64 } else { -1.0 };
        let _ = app.emit(
            event,
            serde_json::json!({ "stage": "download", "percent": percent, "downloaded": downloaded, "total": total }),
        );
        if let Some(st) = status {
            let mut s = st.lock().unwrap();
            if percent >= 0.0 {
                s.percent = span.0 + span.1 * percent;
                s.message = format!(
                    "Downloading… {:.1} / {:.1} MB ({:.0}%)",
                    downloaded as f64 / (1024.0 * 1024.0),
                    total as f64 / (1024.0 * 1024.0),
                    percent * 100.0,
                );
            }
        }
    }
    Ok(())
}

fn extract_archive(archive: &PathBuf, is_zip: bool, dest: &PathBuf) -> Result<(), String> {
    if is_zip {
        let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        z.extract(dest).map_err(|e| e.to_string())?;
    } else {
        let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
        let dec = flate2::read::GzDecoder::new(file);
        let mut ar = tar::Archive::new(dec);
        ar.unpack(dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn chmod_bin(p: &PathBuf) {
    if cfg!(windows) {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    if p.is_file() {
        if let Ok(meta) = std::fs::metadata(p) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(p, perms);
        }
    }
}

#[tauri::command]
async fn setup_engine(app: AppHandle, state: App<'_>, force: Option<bool>) -> Result<serde_json::Value, String> {
    let bin_dir = state.bin_dir.clone();
    let force = force.unwrap_or(false);
    if !force && (binary_present(&bin_dir, "llama-cli") || binary_present(&bin_dir, "llama-server")) {
        return Ok(serde_json::json!({ "ok": true, "installed": true, "skipped": true }));
    }
    let (suffix, is_zip) = llama_asset().ok_or("unsupported platform for automatic llama.cpp download")?;
    let url = llama_url().unwrap();
    let _ = app.emit("engine-progress", serde_json::json!({ "stage": "resolving", "percent": 0.0, "message": url }));
    let ext = if is_zip { "zip" } else { "tar.gz" };
    let tmp = std::env::temp_dir().join(format!("worldsim-llama-{LLAMA_TAG}-{suffix}.{ext}"));
    stream_download(&app, &url, &tmp, "engine-progress", None, (0.0, 1.0)).await?;
    let _ = app.emit("engine-progress", serde_json::json!({ "stage": "extracting", "percent": 1.0 }));
    extract_archive(&tmp, is_zip, &bin_dir).map_err(|e| format!("extract failed: {e}"))?;
    for name in ["llama-cli", "llama-server"] {
        let target = if cfg!(windows) { bin_dir.join(format!("{name}.exe")) } else { bin_dir.join(name) };
        if !target.is_file() {
            for entry in walkdir::WalkDir::new(&bin_dir).into_iter().filter_map(|e| e.ok()) {
                if !entry.file_type().is_file() {
                    continue;
                }
                let f = entry.file_name().to_string_lossy();
                if cfg!(windows) && f == format!("{name}.exe") || !cfg!(windows) && f == name {
                    let _ = std::fs::copy(entry.path(), &target);
                    break;
                }
            }
        }
        chmod_bin(&target);
    }
    let _ = std::fs::remove_file(&tmp);
    let ok = binary_present(&bin_dir, "llama-cli") || binary_present(&bin_dir, "llama-server");
    let _ = app.emit("engine-progress", serde_json::json!({ "stage": "done", "percent": 1.0, "installed": ok }));
    Ok(serde_json::json!({ "ok": true, "installed": ok }))
}

#[tauri::command]
async fn download_model(app: AppHandle, state: App<'_>, url: String, filename: Option<String>, force: Option<bool>) -> Result<serde_json::Value, String> {
    let models_dir = state.models_dir.clone();
    let force = force.unwrap_or(false);
    let fname = filename
        .filter(|s| !s.is_empty())
        .or_else(|| url.rsplit('/').next().map(|s| s.to_string()))
        .ok_or("could not determine filename; pass one explicitly")?;
    let dest = models_dir.join(&fname);
    if dest.is_file() && !force {
        return Ok(serde_json::json!({ "ok": true, "filename": fname, "skipped": true }));
    }
    stream_download(&app, &url, &dest, "model-progress", None, (0.0, 1.0)).await?;
    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    Ok(serde_json::json!({ "ok": true, "filename": fname, "size": size }))
}

#[tauri::command]
fn engine_status(state: App<'_>) -> Result<serde_json::Value, String> {
    let bin_dir = state.bin_dir.clone();
    let models_dir = state.models_dir.clone();
    let cli = binary_present(&bin_dir, "llama-cli");
    let srv = binary_present(&bin_dir, "llama-server");
    let mut models = vec![];
    for spec in worldsim_engine::models::all_models() {
        let p = models_dir.join(spec.filename);
        let present = p.is_file();
        let size = if present { std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) } else { 0 };
        models.push(serde_json::json!({
            "id": spec.id,
            "name": spec.name,
            "filename": spec.filename,
            "present": present,
            "size": size,
        }));
    }
    Ok(serde_json::json!({
        "bin_dir": bin_dir.to_string_lossy().to_string(),
        "models_dir": models_dir.to_string_lossy().to_string(),
        "llama_cli": cli,
        "llama_server": srv,
        "engine_installed": cli || srv,
        "models": models,
        "presets": model_presets(),
    }))
}

#[derive(Default, Clone, Serialize)]
struct SimStatus {
    running: bool,
    percent: f64,
    stage: String,
    message: String,
    log: Vec<String>,
    done: u64,
    total: u64,
}

#[derive(Default, Clone, Serialize)]
struct SetupStatus {
    running: bool,
    stage: String,
    message: String,
    percent: f64,
}

struct AppState {
    engine: Arc<Mutex<Engine>>,
    db_path: PathBuf,
    models_dir: PathBuf,
    bin_dir: PathBuf,
    sim_status: Arc<Mutex<SimStatus>>,
    setup_status: Arc<Mutex<SetupStatus>>,
}

type App<'a> = State<'a, AppState>;

fn resolve() -> (PathBuf, PathBuf, PathBuf) {
    let home = std::env::var("HOME").or_else(|_| std::env::var("APPDATA")).unwrap_or_else(|_| ".".into());
    let base = PathBuf::from(home).join(".worldsim");
    let models = base.join("models");
    let bin = base.join("bin");
    let _ = std::fs::create_dir_all(&models);
    let _ = std::fs::create_dir_all(&bin);
    (
        base.join("worldsim.db"),
        models,
        bin,
    )
}

#[tauri::command]
fn status(state: App<'_>) -> Result<serde_json::Value, String> {
    let e = state.engine.lock().unwrap();
    let models = e.model_status();
    Ok(serde_json::json!({
        "history_start": SimDate::default(),
        "present_year": worldsim_engine::PRESENT_YEAR,
        "eras": worldsim_engine::ERAS,
        "models": models,
        "seed_version": e.storage().get_meta("seed_version").ok().flatten().unwrap_or_default(),
        "canonical_events": e.storage().canonical_event_count().unwrap_or(0),
    }))
}

#[tauri::command]
fn list_scenarios(state: App<'_>) -> Result<Vec<worldsim_engine::storage::Scenario>, String> {
    Ok(state.engine.lock().unwrap().list_scenarios().unwrap_or_default())
}

#[tauri::command]
fn get_scenario(state: App<'_>, id: String) -> Result<Option<worldsim_engine::storage::Scenario>, String> {
    Ok(state.engine.lock().unwrap().get_scenario(&id).unwrap_or(None))
}

#[tauri::command(rename_all = "snake_case")]
fn create_scenario(state: App<'_>, name: String, prompt: String, divergence: String) -> Result<worldsim_engine::storage::Scenario, String> {
    let d = date_from_iso(&divergence).ok_or("bad divergence date")?;
    state.engine.lock().unwrap().create_scenario(&name, &prompt, d).map_err(|e| e.to_string())
}

#[tauri::command]
fn update_scenario(state: App<'_>, id: String, name: Option<String>, prompt: Option<String>) -> Result<(), String> {
    let e = state.engine.lock().unwrap();
    let sc = e.get_scenario(&id).map_err(|e| e.to_string())?.ok_or("not found")?;
    e.storage().update_scenario(&id, &name.unwrap_or(sc.name), &prompt.unwrap_or(sc.prompt)).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_scenario(state: App<'_>, id: String) -> Result<(), String> {
    state.engine.lock().unwrap().storage().delete_scenario(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn branches(state: App<'_>, id: String) -> Result<Vec<worldsim_engine::storage::Branch>, String> {
    Ok(state.engine.lock().unwrap().list_branches(&id).unwrap_or_default())
}

#[tauri::command]
fn world(state: App<'_>, scenario: Option<String>, branch: Option<String>, date: Option<String>) -> Result<serde_json::Value, String> {
    let e = state.engine.lock().unwrap();
    let date = date
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(date_from_iso)
        .unwrap_or_else(|| SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1));
    let snap = e.storage().build_snapshot(date, scenario.as_deref(), branch.as_deref()).unwrap_or_default();
    Ok(serde_json::json!({
        "date": date,
        "snapshot": snap,
        "geojson": snap.to_geojson(),
        "total_population": snap.total_population(),
    }))
}

#[tauri::command]
fn timeline(state: App<'_>, scenario: Option<String>, branch: Option<String>) -> Result<Vec<worldsim_engine::events::HistoryEvent>, String> {
    let e = state.engine.lock().unwrap();
    let to = SimDate { year: 9999, month: 12, day: 28 };
    let canon = e.storage().canonical_events_up_to(to).unwrap_or_default();
    let scn = if let (Some(s), Some(b)) = (scenario, branch) {
        e.storage().scenario_events_up_to(&s, &b, to).unwrap_or_default()
    } else { vec![] };
    Ok(worldsim_engine::apply::merge_sorted(canon, scn))
}

#[tauri::command]
fn compare(state: App<'_>, scenario: String, branch: Option<String>) -> Result<serde_json::Value, String> {
    let e = state.engine.lock().unwrap();
    let date = SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1);
    let canon = e.storage().build_snapshot(date, None, None).unwrap_or_default();
    let scn = e.storage().build_snapshot(date, Some(&scenario), branch.as_deref()).unwrap_or_default();
    let comp = worldsim_engine::scenario::compare_snapshots(&canon, &scn);
    let overlay = worldsim_engine::scenario::overlay_geojson(&canon, &scn);
    Ok(serde_json::json!({ "comparison": comp, "overlay": overlay }))
}

#[tauri::command(rename_all = "snake_case")]
fn simulate(app: AppHandle, state: App<'_>, scenario_id: String, target_date: Option<String>, branch_count: Option<usize>, force_fallback: Option<bool>) -> Result<serde_json::Value, String> {
    let target = target_date.as_deref().and_then(date_from_iso).unwrap_or(SimDate { year: 2100, month: 1, day: 1 });
    let mut opts = worldsim_engine::SimulationOptions::default();
    opts.target_date = target;
    opts.branch_count = branch_count.unwrap_or(1).max(1);
    opts.force_fallback = force_fallback.unwrap_or(false);
    let total = opts.max_steps as u64 * opts.branch_count as u64;
    let status = state.sim_status.clone();
    *status.lock().unwrap() = SimStatus { running: true, percent: 0.0, stage: "start".into(), message: format!("Starting {scenario_id}"), log: vec![], done: 0, total };
    let db = state.db_path.clone();
    let md = state.models_dir.clone();
    let bin = state.bin_dir.clone();
    let sid = scenario_id.clone();
    std::thread::spawn(move || {
        let cb: std::sync::Arc<dyn Fn(SimProgress) + Send + Sync> = std::sync::Arc::new({
            let st = status.clone();
            let app = app.clone();
            move |p: SimProgress| {
                let mut s = st.lock().unwrap();
                s.stage = p.phase.clone();
                let is_step = p.phase == "step" || p.phase == "done";
                if is_step {
                    s.done += 1;
                }
                s.percent = if s.total > 0 { (s.done as f64 / s.total as f64).min(1.0) } else { 0.0 };
                s.message = match p.phase.as_str() {
                    "plan" | "stats" | "validate" => format!("{} ({})", p.note, p.branch_id),
                    _ => format!("{} — {} events ({}): {}", p.branch_id, p.events_created, p.source, p.note),
                };
                let m = s.message.clone();
                s.log.push(m);
                if s.log.len() > 200 { s.log.remove(0); }
                let _ = app.emit("sim-progress", &*s);
            }
        });
        let res = Engine::open(&db, &md, &bin).and_then(|eng| eng.run_scenario(&sid, opts, Some(cb)));
        let mut s = status.lock().unwrap();
        match res {
            Ok(_branches) => { s.running = false; s.percent = 1.0; s.stage = "done".into(); s.message = format!("Finished {sid}"); let m = s.message.clone(); s.log.push(m); }
            Err(e) => { s.running = false; s.stage = "error".into(); s.message = format!("Failed: {e}"); let m = s.message.clone(); s.log.push(m); }
        }
        let _ = app.emit("sim-progress", &*s);
    });
    Ok(serde_json::json!({ "started": true, "scenario_id": scenario_id }))
}

#[tauri::command]
fn simulate_status(state: App<'_>) -> Result<SimStatus, String> {
    Ok(state.sim_status.lock().unwrap().clone())
}

#[tauri::command]
fn refresh_news(state: App<'_>) -> Result<serde_json::Value, String> {
    let e = state.engine.lock().unwrap();
    let st = e.storage();
    if st.list_news_sources().unwrap_or_default().is_empty() {
        for (id, url, trust) in worldsim_engine::news::default_sources() {
            let _ = st.add_news_source(&id, &url, trust);
        }
    }
    let added = worldsim_engine::news::fetch_all(st).unwrap_or(0);
    Ok(serde_json::json!({ "added": added }))
}

#[tauri::command]
fn list_news(state: App<'_>, limit: Option<usize>) -> Result<Vec<worldsim_engine::storage::NewsItemRow>, String> {
    Ok(state.engine.lock().unwrap().storage().top_news_items(limit.unwrap_or(50)).unwrap_or_default())
}

#[tauri::command(rename_all = "snake_case")]
fn seed_news(state: App<'_>, scenario_id: String, branch_id: Option<String>, item_id: Option<String>, limit: Option<usize>) -> Result<serde_json::Value, String> {
    let e = state.engine.lock().unwrap();
    let st = e.storage();
    let br = branch_id.unwrap_or_else(|| "default".into());
    let n = worldsim_engine::news::seed_from_news(
        st,
        &scenario_id,
        &br,
        SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1),
        limit.unwrap_or(20),
    )
    .unwrap_or(0);
    Ok(serde_json::json!({ "seeded": n, "item_id": item_id }))
}

#[tauri::command]
fn setup_status(state: App<'_>) -> Result<SetupStatus, String> {
    Ok(state.setup_status.lock().unwrap().clone())
}

/// One-click, no-button first-run setup: downloads the llama.cpp engine and any
/// missing GGUF models in the background, emitting "setup-progress" events.
#[tauri::command]
async fn ensure_setup(app: AppHandle, state: App<'_>) -> Result<serde_json::Value, String> {
    let bin_dir = state.bin_dir.clone();
    let models_dir = state.models_dir.clone();
    let status = state.setup_status.clone();
    {
        let mut s = status.lock().unwrap();
        if s.running {
            return Ok(serde_json::json!({ "running": true, "message": s.message }));
        }
        s.running = true;
        s.stage = "checking".into();
        s.message = "Checking local engine & models".into();
        s.percent = 0.0;
    }
    let app2 = app.clone();
    let _ = tauri::async_runtime::spawn(async move {
        let result: Result<(), String> = (async {
            if !(binary_present(&bin_dir, "llama-cli") || binary_present(&bin_dir, "llama-server")) {
                set_status(&status, "engine", "Downloading llama.cpp engine…", 0.04);
                let (suffix, is_zip) = llama_asset().ok_or("unsupported platform for automatic llama.cpp download")?;
                let url = llama_url().ok_or("no llama.cpp download url")?;
                let ext = if is_zip { "zip" } else { "tar.gz" };
                let tmp = std::env::temp_dir().join(format!("worldsim-llama-{LLAMA_TAG}-{suffix}.{ext}"));
                stream_download(&app2, &url, &tmp, "setup-progress", Some(&status), (0.0, 0.45)).await?;
                set_status(&status, "engine", "Extracting llama.cpp…", 0.45);
                extract_archive(&tmp, is_zip, &bin_dir)?;
                for name in ["llama-cli", "llama-server"] {
                    let target = if cfg!(windows) { bin_dir.join(format!("{name}.exe")) } else { bin_dir.join(name) };
                    if !target.is_file() {
                        for entry in walkdir::WalkDir::new(&bin_dir).into_iter().filter_map(|e| e.ok()) {
                            if !entry.file_type().is_file() {
                                continue;
                            }
                            let f = entry.file_name().to_string_lossy();
                            if (cfg!(windows) && f == format!("{name}.exe")) || (!cfg!(windows) && f == name) {
                                let _ = std::fs::copy(entry.path(), &target);
                                break;
                            }
                        }
                    }
                    chmod_bin(&target);
                }
                let _ = std::fs::remove_file(&tmp);
            }
            let models = worldsim_engine::models::all_models();
            let total = models.len();
            for (i, spec) in models.iter().enumerate() {
                let p = models_dir.join(spec.filename);
                if p.is_file() {
                    continue;
                }
                let (url, _fname) = preset_for(spec.id).ok_or_else(|| format!("no preset url for {}", spec.id))?;
                let msg = format!("Downloading {} ({})…", spec.name, spec.base_model);
                let base = 0.5 + 0.5 * (i as f64 / total as f64);
                let span = 0.5 / total as f64;
                set_status(&status, spec.id, &msg, base);
                stream_download(&app2, &url, &p, "setup-progress", Some(&status), (base, span)).await?;
            }
            Ok(())
        })
        .await;
        let mut s = status.lock().unwrap();
        s.running = false;
        s.stage = "done".into();
        s.message = match result {
            Ok(()) => "Setup complete. All local AI components are ready.".into(),
            Err(e) => format!("Setup failed: {e}"),
        };
        s.percent = 1.0;
    });
    Ok(serde_json::json!({ "running": true }))
}

pub fn run() {
    let (db, models, bin) = resolve();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let engine = Engine::open(&db, &models, &bin)
                .map_err(|e| {
                    eprintln!("engine open failed: {e}");
                    std::process::exit(1);
                })
                .unwrap();

            if let Some(seed) = find_seed_db(app) {
                match engine.storage().seed_canonical_from(&seed) {
                    Ok(n) if n > 0 => {
                        eprintln!("seeded {n} canonical events from {}", seed.display());
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("canonical seed import failed: {e}"),
                }
            }

            let state = AppState {
                engine: Arc::new(Mutex::new(engine)),
                db_path: db,
                models_dir: models,
                bin_dir: bin,
                sim_status: Arc::new(Mutex::new(SimStatus::default())),
                setup_status: Arc::new(Mutex::new(SetupStatus::default())),
            };
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            status, list_scenarios, get_scenario, create_scenario, update_scenario,
            delete_scenario, branches, world, timeline, compare, simulate,
            simulate_status, refresh_news, list_news, seed_news,
            setup_engine, download_model, engine_status,
            setup_status, ensure_setup
        ])
        .run(tauri::generate_context!())
        .expect("error while running WorldSimulator");
}

/// Locate a canonical seed DB: explicit env override first, then the bundled
/// resource (release builds), then repo-local dev copies.
fn find_seed_db(app: &tauri::App) -> Option<PathBuf> {
    if let Ok(p) = std::env::var("WSIM_SEED_DB") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(dir) = app.path().resource_dir() {
        let p = dir.join("worldsim.db");
        if p.is_file() {
            return Some(p);
        }
    }
    for rel in ["../data/out/worldsim.db", "data/out/worldsim.db"] {
        let p = PathBuf::from(rel);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}
