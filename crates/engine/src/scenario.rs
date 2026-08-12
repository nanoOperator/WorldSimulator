//! Scenario operations: parallel branch execution, editing, comparison, and
//! map overlay generation.

use crate::engine::{run_branch, SimulationOptions};
use crate::llm::LlamaClient;
use crate::state::WorldSnapshot;
use crate::storage::{Branch, Scenario, Storage};
use crate::{EngineError, Result, SimDate};
use std::path::PathBuf;

pub type ProgressCb = crate::engine::ProgressCb;

/// Spawn one thread per branch. Each thread opens its own SQLite connection
/// (WAL mode) and its own llama.cpp client, so branches run in parallel.
pub fn run_scenario_branches(
    db_path: PathBuf,
    models_dir: PathBuf,
    bin_dir: PathBuf,
    scenario: Scenario,
    branches: Vec<Branch>,
    options: SimulationOptions,
    progress: Option<ProgressCb>,
) -> Result<Vec<Branch>> {
    let mut handles = Vec::new();
    for branch in branches.clone() {
        let db = db_path.clone();
        let md = models_dir.clone();
        let bd = bin_dir.clone();
        let sc = scenario.clone();
        let opts = options.clone();
        let prog = progress.clone();
        handles.push(std::thread::spawn(move || -> Result<usize> {
            let storage = Storage::open(&db)
                .map_err(|e| EngineError::Storage(format!("branch open: {e}")))?;
            let llm = LlamaClient::new(&md, &bd);
            run_branch(&storage, &llm, &sc, &branch, &opts, &prog)
        }));
    }
    for h in handles {
        h.join()
            .map_err(|e| EngineError::Storage(format!("branch thread panicked: {e:?}")))??;
    }
    Ok(branches)
}

/// Edit a scenario's name/prompt (keeps existing events; re-run to extend).
pub fn edit_scenario(storage: &Storage, id: &str, name: &str, prompt: &str) -> Result<()> {
    storage.update_scenario(id, name, prompt)
}

// ------------------------------------------------------------------ compare

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TerritoryDiff {
    pub territory: String,
    pub canonical_owner: String,
    pub scenario_owner: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatDiff {
    pub nation: String,
    pub canonical_population: i64,
    pub scenario_population: i64,
    pub delta_population: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorldComparison {
    pub date: SimDate,
    pub territory_diffs: Vec<TerritoryDiff>,
    pub stat_diffs: Vec<StatDiff>,
    pub invented_techs: Vec<String>,
    /// Human-readable summary of every divergence, ready for the UI.
    pub changes: Vec<String>,
}

/// Diff the canonical world against a scenario world at the same date.
pub fn compare_snapshots(canonical: &WorldSnapshot, scenario: &WorldSnapshot) -> WorldComparison {
    let mut territory_diffs = Vec::new();
    let canonical_by_id: std::collections::HashMap<&str, &crate::state::Territory> =
        canonical.territories.iter().map(|t| (t.id.as_str(), t)).collect();
    let scenario_by_id: std::collections::HashMap<&str, &crate::state::Territory> =
        scenario.territories.iter().map(|t| (t.id.as_str(), t)).collect();
    let mut all_ids: Vec<&str> = canonical_by_id
        .keys()
        .chain(scenario_by_id.keys())
        .copied()
        .collect();
    all_ids.sort();
    all_ids.dedup();
    for id in all_ids {
        let c = canonical_by_id.get(id).map(|t| t.owner.as_str()).unwrap_or("(does not exist)");
        let s = scenario_by_id.get(id).map(|t| t.owner.as_str()).unwrap_or("(does not exist)");
        if c != s {
            territory_diffs.push(TerritoryDiff {
                territory: id.to_string(),
                canonical_owner: c.to_string(),
                scenario_owner: s.to_string(),
            });
        }
    }

    let mut stat_diffs = Vec::new();
    for sn in &scenario.nations {
        let cp = canonical.nation(&sn.id).map(|n| n.population).unwrap_or(0);
        if cp != sn.population {
            stat_diffs.push(StatDiff {
                nation: sn.id.clone(),
                canonical_population: cp,
                scenario_population: sn.population,
                delta_population: sn.population - cp,
            });
        }
    }

    let canon_techs: std::collections::HashSet<&str> =
        canonical.techs.iter().map(|t| t.tech_id.as_str()).collect();
    let invented_techs: Vec<String> = scenario
        .techs
        .iter()
        .filter(|t| !canon_techs.contains(t.tech_id.as_str()))
        .map(|t| t.name.clone())
        .collect();

    let mut changes: Vec<String> = Vec::new();
    for d in &territory_diffs {
        changes.push(format!(
            "Border: {} changed from {} to {}",
            d.territory, d.canonical_owner, d.scenario_owner
        ));
    }
    for s in &stat_diffs {
        changes.push(format!(
            "Population: {} {}{} (was {})",
            s.nation,
            if s.delta_population >= 0 { "+" } else { "" },
            s.delta_population,
            s.canonical_population
        ));
    }
    for t in &invented_techs {
        changes.push(format!("New technology: {t}"));
    }

    WorldComparison {
        date: scenario.date,
        territory_diffs,
        stat_diffs,
        invented_techs,
        changes,
    }
}

/// Overlay GeoJSON: scenario territories in full color plus canonical
/// borders as a translucent outline layer.
pub fn overlay_geojson(canonical: &WorldSnapshot, scenario: &WorldSnapshot) -> serde_json::Value {
    let mut features = Vec::new();
    let color = |owner: &str| -> String {
        scenario
            .nation(owner)
            .map(|n| n.color.clone())
            .unwrap_or("#888888".into())
    };
    for t in &scenario.territories {
        features.push(feature_json(
            t.id.as_str(),
            &t.name,
            &color(&t.owner),
            &t.geometry_geojson,
            0.9,
            false,
        ));
    }
    for t in &canonical.territories {
        features.push(feature_json(
            t.id.as_str(),
            &t.name,
            "#222222",
            &t.geometry_geojson,
            0.35,
            true,
        ));
    }
    serde_json::json!({ "type": "FeatureCollection", "features": features })
}

fn feature_json(
    id: &str,
    name: &str,
    color: &str,
    geometry: &str,
    opacity: f64,
    outline_only: bool,
) -> serde_json::Value {
    serde_json::json!({
        "type": "Feature",
        "properties": {
            "id": id,
            "name": name,
            "color": color,
            "opacity": opacity,
            "outlineOnly": outline_only,
        },
        "geometry": serde_json::from_str::<serde_json::Value>(geometry)
            .unwrap_or(serde_json::json!(null)),
    })
}

/// Convenience: full comparison result as a flat map for the UI.
pub fn compare(
    storage: &Storage,
    date: SimDate,
    scenario_id: Option<&str>,
    branch_id: Option<&str>,
) -> Result<WorldComparison> {
    let canonical = storage.build_snapshot(date, None, None)?;
    let scenario = storage.build_snapshot(date, scenario_id, branch_id)?;
    Ok(compare_snapshots(&canonical, &scenario))
}
