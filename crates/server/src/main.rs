//! WorldSimulator local HTTP server.
//!
//! Serves the engine API plus the static web frontend. Run with:
//!   WSIM_DB=~/.worldsim/world.db WSIM_MODELS=models WSIM_BIN=/usr/local/bin \
//!   WSIM_STATIC=app/dist cargo run --bin worldsim-server
//!
//! The simulation endpoint returns immediately and runs branches in the
//! background; the UI polls branch status and events.

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use worldsim_engine::events::HistoryEvent;
use worldsim_engine::state::WorldSnapshot;
use worldsim_engine::storage::Scenario;
use worldsim_engine::{date_from_iso, Engine, SimDate};

#[derive(Debug, Default, Clone, serde::Serialize)]
struct SimStatus {
    running: bool,
    percent: f64,
    stage: String,
    message: String,
    log: Vec<String>,
    done: u64,
    total: u64,
}

struct AppState {
    engine: Arc<Mutex<Engine>>,
    db_path: PathBuf,
    models_dir: PathBuf,
    bin_dir: PathBuf,
    sim_status: Arc<Mutex<SimStatus>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,worldsim=debug")
        .init();

    let db_path = env_path("WSIM_DB", "worldsim.db");
    // Default to the shared install dir the desktop app uses, so a bare
    // `cargo run -p worldsim-server` finds real models and llama.cpp.
    let home = std::env::var("HOME").or_else(|_| std::env::var("APPDATA")).unwrap_or_default();
    let models_dir = env_path("WSIM_MODELS", &format!("{home}/.worldsim/models"));
    let bin_dir = env_path("WSIM_BIN", &format!("{home}/.worldsim/bin"));
    let static_dir = std::env::var("WSIM_STATIC").ok();
    let port: u16 = std::env::var("WSIM_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(7676);

    std::fs::create_dir_all(db_path.parent().unwrap_or(std::path::Path::new(".")))
        .unwrap_or_else(|e| eprintln!("note: could not create db dir: {e}"));
    std::fs::create_dir_all(&models_dir).unwrap_or_else(|e| eprintln!("note: could not create models dir: {e}"));

    let engine = Engine::open(&db_path, &models_dir, &bin_dir)
        .map_err(|e| {
            eprintln!("failed to open engine: {e}");
            std::process::exit(1);
        })
        .unwrap();

    // Auto-import the canonical timeline on first run so the server's world is
    // never empty. Override with WSIM_SEED_DB.
    let seed_path = std::env::var("WSIM_SEED_DB").unwrap_or_else(|_| "data/out/worldsim.db".into());
    match engine.storage().seed_canonical_from(&seed_path) {
        Ok(n) if n > 0 => tracing::info!("seeded {n} canonical events from {seed_path}"),
        Ok(_) => {}
        Err(e) => eprintln!("note: canonical seed import failed: {e}"),
    }

    let state = Arc::new(AppState {
        engine: Arc::new(Mutex::new(engine)),
        db_path,
        models_dir,
        bin_dir,
        sim_status: Arc::new(Mutex::new(SimStatus::default())),
    });

    let app = Router::new()
        .route("/api/status", get(status))
        .route("/api/scenarios", get(list_scenarios).post(create_scenario))
        .route("/api/scenarios/:id", post(update_scenario).delete(delete_scenario))
        .route("/api/scenarios/:id/branches", get(list_branches))
        .route("/api/simulate", post(simulate))
        .route("/api/simulate/status", get(simulate_status))
        .route("/api/world", get(world))
        .route("/api/timeline", get(timeline))
        .route("/api/compare", get(compare))
        .route("/api/news/refresh", post(news_refresh))
        .route("/api/news", get(list_news))
        .route("/api/news/seed", post(news_seed))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let app = if let Some(dir) = static_dir {
        let dist = tower_http::services::ServeDir::new(&dir);
        app.fallback_service(dist)
    } else {
        app
    };

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("WorldSimulator server on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

fn env_path(key: &str, default: &str) -> PathBuf {
    std::env::var(key)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

// ---------------------------------------------------------------- status

async fn status(State(st): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let engine = st.engine.lock().unwrap();
    let models = engine.model_status();
    let seed_version = engine
        .storage()
        .get_meta("seed_version")
        .ok()
        .flatten()
        .unwrap_or_else(|| "none".into());
    Json(serde_json::json!({
        "history_start": SimDate::default(),
        "present_year": worldsim_engine::PRESENT_YEAR,
        "eras": worldsim_engine::ERAS,
        "models": models,
        "seed_version": seed_version,
        "canonical_events": engine.storage().canonical_event_count().unwrap_or(0),
    }))
}

// -------------------------------------------------------------- scenarios

#[derive(Deserialize)]
struct CreateScenario {
    name: String,
    prompt: String,
    divergence: String,
}

async fn create_scenario(
    State(st): State<Arc<AppState>>,
    Json(req): Json<CreateScenario>,
) -> Result<Json<Scenario>, (StatusCode, String)> {
    let divergence = date_from_iso(&req.divergence)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("bad date '{}'", req.divergence)))?;
    let engine = st.engine.lock().unwrap();
    let sc = engine
        .create_scenario(&req.name, &req.prompt, divergence)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(sc))
}

async fn list_scenarios(State(st): State<Arc<AppState>>) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let engine = st.engine.lock().unwrap();
    let scenarios = engine.list_scenarios().map_err(err500)?;
    let storage = engine.storage();
    let mut out = Vec::new();
    for s in scenarios {
        let branches = storage.branch_count(&s.id).unwrap_or(0);
        out.push(serde_json::json!({
            "id": s.id,
            "name": s.name,
            "prompt": s.prompt,
            "divergence": s.divergence,
            "created_at": s.created_at,
            "branches": branches,
        }));
    }
    Ok(Json(out))
}

async fn list_branches(
    State(st): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let engine = st.engine.lock().unwrap();
    let branches = engine.list_branches(&id).map_err(err500)?;
    let storage = engine.storage();
    let mut out = Vec::new();
    for (i, b) in branches.iter().enumerate() {
        let (event_count, final_date) = storage
            .branch_event_stats(&id, &b.id)
            .map_err(err500)?;
        out.push(serde_json::json!({
            "id": b.id,
            "scenario_id": b.scenario_id,
            "parent_id": b.parent_id,
            "seed": b.seed,
            "status": b.status,
            "created_at": b.created_at,
            "label": format!("Branch {}", i + 1),
            "event_count": event_count,
            "final_date": final_date.map(|d| d.display_year()),
        }));
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
struct UpdateScenario {
    name: Option<String>,
    prompt: Option<String>,
}

async fn update_scenario(
    State(st): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateScenario>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = st.engine.lock().unwrap();
    let sc = engine
        .get_scenario(&id)
        .map_err(err500)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "scenario not found".into()))?;
    let name = req.name.unwrap_or(sc.name);
    let prompt = req.prompt.unwrap_or(sc.prompt);
    engine
        .storage()
        .update_scenario(&id, &name, &prompt)
        .map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn delete_scenario(
    State(st): State<Arc<AppState>>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = st.engine.lock().unwrap();
    engine.storage().delete_scenario(&id).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// --------------------------------------------------------------- simulate

#[derive(Deserialize)]
struct SimulateReq {
    scenario_id: String,
    #[serde(default = "default_target")]
    target_date: String,
    #[serde(default = "default_branches")]
    branch_count: usize,
    #[serde(default)]
    force_fallback: bool,
}

fn default_target() -> String {
    "2100".to_string()
}
fn default_branches() -> usize {
    1
}

async fn simulate(
    State(st): State<Arc<AppState>>,
    Json(req): Json<SimulateReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let target = date_from_iso(&req.target_date)
        .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("bad date '{}'", req.target_date)))?;
    // Snapshot the paths, then run outside the lock.
    let db_path = st.db_path.clone();
    let models_dir = st.models_dir.clone();
    let bin_dir = st.bin_dir.clone();
    let engine = st.engine.lock().unwrap();
    let scenario = engine
        .get_scenario(&req.scenario_id)
        .map_err(err500)?
        .ok_or_else(|| (StatusCode::NOT_FOUND, "scenario not found".into()))?;
    drop(engine);

    let options = worldsim_engine::SimulationOptions {
        target_date: target,
        branch_count: req.branch_count.max(1),
        force_fallback: req.force_fallback,
        ..Default::default()
    };

    let scenario_id = scenario.id.clone();
    let scenario_id_for_json = scenario_id.clone();
    let status = st.sim_status.clone();
    let total = options.max_steps as u64 * options.branch_count as u64;
    *status.lock().unwrap() = SimStatus {
        running: true,
        percent: 0.0,
        stage: "start".into(),
        message: format!("Starting simulation for scenario {scenario_id}"),
        log: vec![],
        done: 0,
        total,
    };
    std::thread::spawn(move || -> std::result::Result<(), String> {
        let cb_status = status.clone();
        let cb = std::sync::Arc::new(move |p: worldsim_engine::SimProgress| {
            let mut s = cb_status.lock().unwrap();
            s.stage = p.phase.clone();
            if p.phase == "step" || p.phase == "done" {
                s.done += 1;
            }
            s.percent = if s.total > 0 { (s.done as f64 / s.total as f64).min(1.0) } else { 0.0 };
            s.message = match p.phase.as_str() {
                "plan" | "stats" | "validate" => format!("{} ({})", p.note, p.branch_id),
                _ => format!("{} — {} events ({}): {}", p.branch_id, p.events_created, p.source, p.note),
            };
            let msg = s.message.clone();
            s.log.push(msg);
            if s.log.len() > 200 {
                s.log.remove(0);
            }
        });
        let engine = Engine::open(&db_path, &models_dir, &bin_dir).map_err(|e| e.to_string())?;
        let res = engine.run_scenario(&scenario_id, options, Some(cb));
        let mut s = status.lock().unwrap();
        match res {
            Ok(_branches) => {
                s.running = false;
                s.percent = 1.0;
                s.stage = "done".into();
                s.message = format!("Simulation of {scenario_id} finished");
            }
            Err(e) => {
                s.running = false;
                s.stage = "error".into();
                s.message = format!("Simulation failed: {e}");
                let msg = s.message.clone();
                s.log.push(msg);
            }
        }
        tracing::info!("simulation of {} finished", scenario_id);
        Ok(())
    });

    Ok(Json(serde_json::json!({ "started": true, "scenario_id": scenario_id_for_json })))
}

async fn simulate_status(
    State(st): State<Arc<AppState>>,
) -> Json<SimStatus> {
    Json(st.sim_status.lock().unwrap().clone())
}

// ------------------------------------------------------------------ world

#[derive(Deserialize)]
struct WorldQuery {
    date: Option<String>,
    scenario: Option<String>,
    branch: Option<String>,
}

async fn world(
    State(st): State<Arc<AppState>>,
    Query(q): Query<WorldQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let date = q
        .date
        .as_deref()
        .and_then(date_from_iso)
        .unwrap_or_else(|| SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1));
    let engine = st.engine.lock().unwrap();
    let snapshot: WorldSnapshot = engine
        .storage()
        .build_snapshot(date, q.scenario.as_deref(), q.branch.as_deref())
        .map_err(err500)?;
    let geojson = snapshot.to_geojson();
    Ok(Json(serde_json::json!({
        "date": date,
        "snapshot": snapshot,
        "geojson": geojson,
        "total_population": snapshot.total_population(),
    })))
}

// --------------------------------------------------------------- timeline

#[derive(Deserialize)]
struct TimelineQuery {
    scenario: Option<String>,
    branch: Option<String>,
    from: Option<String>,
    to: Option<String>,
}

async fn timeline(
    State(st): State<Arc<AppState>>,
    Query(q): Query<TimelineQuery>,
) -> Result<Json<Vec<HistoryEvent>>, (StatusCode, String)> {
    let from = q.from.as_deref().and_then(date_from_iso).unwrap_or_default();
    let to = q
        .to
        .as_deref()
        .and_then(date_from_iso)
        .unwrap_or(SimDate { year: 9999, month: 12, day: 28 });
    let engine = st.engine.lock().unwrap();
    let storage = engine.storage();

    let mut canonical = storage.canonical_events_up_to(to).map_err(err500)?;
    canonical.retain(|e| e.date >= from);

    let mut scenario = Vec::new();
    if let (Some(sc), Some(br)) = (&q.scenario, &q.branch) {
        scenario = storage
            .scenario_events_up_to(sc, br, to)
            .map_err(err500)?;
        scenario.retain(|e| e.date >= from);
    }

    let merged = worldsim_engine::apply::merge_sorted(canonical, scenario);
    Ok(Json(merged))
}

// ---------------------------------------------------------------- compare

#[derive(Deserialize)]
struct CompareQuery {
    scenario: String,
    branch: Option<String>,
    date: Option<String>,
}

async fn compare(
    State(st): State<Arc<AppState>>,
    Query(q): Query<CompareQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let date = q.date.as_deref().and_then(date_from_iso).unwrap_or_else(|| SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1));
    let engine = st.engine.lock().unwrap();
    let storage = engine.storage();
    let canonical = storage.build_snapshot(date, None, None).map_err(err500)?;
    let scenario = storage
        .build_snapshot(date, Some(&q.scenario), q.branch.as_deref())
        .map_err(err500)?;
    let comparison = worldsim_engine::scenario::compare_snapshots(&canonical, &scenario);
    let overlay = worldsim_engine::scenario::overlay_geojson(&canonical, &scenario);
    Ok(Json(serde_json::json!({
        "comparison": comparison,
        "overlay": overlay,
    })))
}

// ------------------------------------------------------------------- news

async fn news_refresh(State(st): State<Arc<AppState>>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db_path = st.db_path.clone();
    {
        let engine = st.engine.lock().unwrap();
        let storage = engine.storage();
        if storage.list_news_sources().map_err(err500)?.is_empty() {
            for (id, url, trust) in worldsim_engine::news::default_sources() {
                storage.add_news_source(&id, &url, trust).map_err(err500)?;
            }
        }
    }
    // Fetch feeds on a background thread so the engine lock is never held for
    // the (up to ~80s) network work, and so blocking reqwest calls never run
    // inside the tokio async runtime (which panics).
    std::thread::spawn(move || {
        if let Ok(storage) = worldsim_engine::storage::Storage::open(&db_path) {
            let _ = worldsim_engine::news::fetch_all(&storage);
        }
    });
    Ok(Json(serde_json::json!({ "started": true })))
}

#[derive(Deserialize)]
struct NewsQuery {
    limit: Option<usize>,
}

async fn list_news(
    State(st): State<Arc<AppState>>,
    Query(q): Query<NewsQuery>,
) -> Result<Json<Vec<worldsim_engine::storage::NewsItemRow>>, (StatusCode, String)> {
    let engine = st.engine.lock().unwrap();
    let items = engine.storage().top_news_items(q.limit.unwrap_or(50)).map_err(err500)?;
    Ok(Json(items))
}

#[derive(Deserialize)]
struct NewsSeedReq {
    scenario_id: String,
    branch_id: Option<String>,
    limit: Option<usize>,
}

async fn news_seed(
    State(st): State<Arc<AppState>>,
    Json(req): Json<NewsSeedReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let engine = st.engine.lock().unwrap();
    let branch_id = req.branch_id.unwrap_or_else(|| "default".into());
    let n = worldsim_engine::news::seed_from_news(
        engine.storage(),
        &req.scenario_id,
        &branch_id,
        SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1),
        req.limit.unwrap_or(20),
    )
    .map_err(err500)?;
    Ok(Json(serde_json::json!({ "seeded": n })))
}

fn err500(e: worldsim_engine::EngineError) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}
