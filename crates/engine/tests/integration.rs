use worldsim_engine::engine::Engine;
use worldsim_engine::date_from_iso;
use worldsim_engine::EngineError;

fn seed_db_path() -> String {
    // crates/engine -> repo root -> data/out/worldsim.db
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(manifest);
    repo.join("data")
        .join("out")
        .join("worldsim.db")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn full_pipeline_with_seed_db() -> Result<(), EngineError> {
    let db = seed_db_path();
    let engine = Engine::open(&db, "models", "")?;
    let count = engine.storage().canonical_event_count()?;
    assert!(count > 50, "seed events missing: {count}");

    let snap_1938 =
        engine
            .storage()
            .build_snapshot(date_from_iso("1938-01-01").unwrap(), None, None)?;
    assert!(!snap_1938.territories.is_empty(), "no territories at 1938");
    assert!(!snap_1938.nations.is_empty(), "no nations at 1938");

    let sc = engine.create_scenario(
        "Nazi 1943",
        "What if the Nazis won in 1943?",
        date_from_iso("1943-06-01").unwrap(),
    )?;
    let branches = engine.run_scenario(
        &sc.id,
        worldsim_engine::SimulationOptions {
            branch_count: 1,
            target_date: date_from_iso("1970-01-01").unwrap(),
            force_fallback: true,
            ..Default::default()
        },
        None,
    )?;
    assert_eq!(branches.len(), 1);

    let snap = engine.storage().build_snapshot(
        date_from_iso("1970-01-01").unwrap(),
        Some(&sc.id),
        Some(&branches[0].id),
    )?;
    assert!(!snap.nations.is_empty());
    Ok(())
}

#[test]
#[ignore = "requires ~/.worldsim/bin and ~/.worldsim/models"]
fn live_llm_pipeline_with_models() -> Result<(), EngineError> {
    let home = std::env::var("HOME").expect("HOME");
    let base = std::path::PathBuf::from(&home).join(".worldsim");
    let models = base.join("models");
    let bin = base.join("bin");
    let db = seed_db_path();

    let engine = Engine::open(&db, &models, &bin)?;
    let sc = engine.create_scenario(
        "Live Nazi WWII",
        "What if the Nazis won WWII?",
        date_from_iso("1945-05-01").unwrap(),
    )?;
    let branches = engine.run_scenario(
        &sc.id,
        worldsim_engine::SimulationOptions {
            branch_count: 1,
            target_date: date_from_iso("1947-01-01").unwrap(),
            max_steps: 3,
            force_fallback: false,
            ..Default::default()
        },
        None,
    )?;
    assert_eq!(branches.len(), 1);
    let (event_count, _) = engine.storage().branch_event_stats(&sc.id, &branches[0].id)?;
    println!("Live simulation created {} events!", event_count);
    assert!(event_count > 0);
    Ok(())
}
