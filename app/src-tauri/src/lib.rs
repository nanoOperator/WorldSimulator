use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::State;

use worldsim_engine::{date_from_iso, Engine, SimDate, SimProgress};

#[derive(Default, Clone, Serialize)]
struct SimStatus {
    running: bool,
    percent: f64,
    stage: String,
    message: String,
    log: Vec<String>,
}

struct AppState {
    engine: Arc<Mutex<Engine>>,
    db_path: PathBuf,
    models_dir: PathBuf,
    bin_dir: PathBuf,
    sim_status: Arc<Mutex<SimStatus>>,
}

type App<'a> = State<'a, AppState>;

fn resolve() -> (PathBuf, PathBuf, PathBuf) {
    let home = std::env::var("HOME").or_else(|_| std::env::var("APPDATA")).unwrap_or_else(|_| ".".into());
    let base = PathBuf::from(home).join(".worldsim");
    let _ = std::fs::create_dir_all(&base);
    (
        base.join("worldsim.db"),
        base.join("models"),
        PathBuf::from(""),
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

#[tauri::command]
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
fn world(state: App<'_>, scenario: Option<String>, branch: Option<String>) -> Result<serde_json::Value, String> {
    let e = state.engine.lock().unwrap();
    let date = SimDate::from_ce(worldsim_engine::PRESENT_YEAR, 1, 1);
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

#[tauri::command]
fn simulate(state: App<'_>, scenario_id: String, target_date: Option<String>, branch_count: Option<usize>) -> Result<serde_json::Value, String> {
    let target = target_date.as_deref().and_then(date_from_iso).unwrap_or(SimDate { year: 2100, month: 1, day: 1 });
    let mut opts = worldsim_engine::SimulationOptions::default();
    opts.target_date = target;
    opts.branch_count = branch_count.unwrap_or(1).max(1);
    let status = state.sim_status.clone();
    *status.lock().unwrap() = SimStatus { running: true, percent: 0.0, stage: "start".into(), message: format!("Starting {scenario_id}"), log: vec![] };
    let db = state.db_path.clone();
    let md = state.models_dir.clone();
    let bin = state.bin_dir.clone();
    let sid = scenario_id.clone();
    std::thread::spawn(move || {
        let cb: std::sync::Arc<dyn Fn(SimProgress) + Send + Sync> = std::sync::Arc::new({
            let st = status.clone();
            move |p: SimProgress| {
                let mut s = st.lock().unwrap();
                s.percent = (p.step as f64).max(s.percent);
                s.stage = p.source.clone();
                s.message = format!("{} | branch {} @ {} ({} events)", p.note, p.branch_id, p.date, p.events_created);
                let m = s.message.clone();
                s.log.push(m);
                if s.log.len() > 200 { s.log.remove(0); }
            }
        });
        let res = Engine::open(&db, &md, &bin).and_then(|eng| eng.run_scenario(&sid, opts, Some(cb)));
        let mut s = status.lock().unwrap();
        match res {
            Ok(_branches) => { s.running = false; s.percent = 1.0; s.stage = "done".into(); s.message = format!("Finished {sid}"); }
            Err(e) => { s.running = false; s.stage = "error".into(); s.message = format!("Failed: {e}"); let m = s.message.clone(); s.log.push(m); }
        }
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

#[tauri::command]
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

pub fn run() {
    let (db, models, bin) = resolve();
    let engine = Engine::open(&db, &models, &bin)
        .map_err(|e| { eprintln!("engine open failed: {e}"); std::process::exit(1); })
        .unwrap();

    let state = AppState {
        engine: Arc::new(Mutex::new(engine)),
        db_path: db,
        models_dir: models,
        bin_dir: bin,
        sim_status: Arc::new(Mutex::new(SimStatus::default())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            status, list_scenarios, get_scenario, create_scenario, update_scenario,
            delete_scenario, branches, world, timeline, compare, simulate,
            simulate_status, refresh_news, list_news, seed_news
        ])
        .run(tauri::generate_context!())
        .expect("error while running WorldSimulator");
}
