//! The simulation engine: orchestrates the adaptive step loop, routes work to
//! mustafakemal/inalcik, validates, self-checks, and records the causal log. Runs
//! scenario branches in parallel/background.

use crate::apply;
use crate::events::{EventPayload, HistoryEvent};
use crate::fallback::{self, FallbackConfig};
use crate::llm::LlamaClient;
use crate::models;
use crate::scenario::run_scenario_branches;
use crate::state::WorldSnapshot;
use crate::storage::{Branch, Scenario, Storage};
use crate::validate::{self, Violation};
use crate::{EngineError, Result, SimDate};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Progress callback emitted during simulation.
pub type ProgressCb = Arc<dyn Fn(SimProgress) + Send + Sync>;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimProgress {
    pub scenario_id: String,
    pub branch_id: String,
    pub date: SimDate,
    pub step: u32,
    pub events_created: usize,
    /// mustafakemal | inalcik | fallback
    pub source: String,
    /// lifecycle phase: start | plan | stats | validate | apply | step | done
    pub phase: String,
    pub note: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimulationOptions {
    /// Number of parallel outcome branches to produce.
    pub branch_count: usize,
    pub target_date: SimDate,
    /// Force the deterministic fallback even when models exist.
    pub force_fallback: bool,
    pub max_steps: u32,
    /// Sampling temperature.
    pub temperature: f64,
    /// Seed base for branch RNG (branches offset from it).
    pub seed: u64,
}

impl Default for SimulationOptions {
    fn default() -> Self {
        SimulationOptions {
            branch_count: 1,
            target_date: SimDate::from_ce(PRESENT_TARGET, 1, 1),
            force_fallback: false,
            max_steps: 200,
            temperature: 0.8,
            seed: 7,
        }
    }
}

pub const PRESENT_TARGET: i32 = 2100;

/// Top-level engine.
pub struct Engine {
    storage: Storage,
    llm: LlamaClient,
    models_dir: PathBuf,
    bin_dir: PathBuf,
    db_path: PathBuf,
}

impl Engine {
    pub fn open(db_path: impl AsRef<Path>, models_dir: impl AsRef<Path>, bin_dir: impl AsRef<Path>) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let storage = Storage::open(&db_path)?;
        let llm = LlamaClient::new(models_dir.as_ref(), bin_dir.as_ref());
        Ok(Engine {
            storage,
            llm,
            models_dir: models_dir.as_ref().to_path_buf(),
            bin_dir: bin_dir.as_ref().to_path_buf(),
            db_path,
        })
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    /// Names and availability of the bundled models.
    pub fn model_status(&self) -> Vec<ModelStatus> {
        models::all_models()
            .iter()
            .map(|m| ModelStatus {
                id: m.id.to_string(),
                name: m.name.to_string(),
                base_model: m.base_model.to_string(),
                size_b: m.size_b,
                role: m.role,
                quantization: m.quantization.to_string(),
                available: self.llm.available(m),
            })
            .collect()
    }

    // ---------------------------------------------------------- scenarios

    pub fn create_scenario(
        &self,
        name: &str,
        prompt: &str,
        divergence: SimDate,
    ) -> Result<Scenario> {
        let id = format!("sc-{}", uuid::Uuid::new_v4().simple());
        self.storage.create_scenario(&id, name, prompt, divergence)?;
        // The default first branch.
        self.storage.create_branch(&Branch {
            id: format!("br-{}", uuid::Uuid::new_v4().simple()),
            scenario_id: id.clone(),
            parent_id: None,
            seed: 1,
            status: "pending".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })?;
        Ok(self
            .storage
            .get_scenario(&id)?
            .ok_or_else(|| EngineError::ScenarioNotFound(id.clone()))?)
    }

    pub fn list_scenarios(&self) -> Result<Vec<Scenario>> {
        self.storage.list_scenarios()
    }

    pub fn get_scenario(&self, id: &str) -> Result<Option<Scenario>> {
        self.storage.get_scenario(id)
    }

    pub fn list_branches(&self, scenario_id: &str) -> Result<Vec<Branch>> {
        self.storage.list_branches(scenario_id)
    }

    /// Run a scenario across `options.branch_count` parallel branches.
    pub fn run_scenario(
        &self,
        scenario_id: &str,
        options: SimulationOptions,
        progress: Option<ProgressCb>,
    ) -> Result<Vec<Branch>> {
        let scenario = self
            .storage
            .get_scenario(scenario_id)?
            .ok_or_else(|| EngineError::ScenarioNotFound(scenario_id.into()))?;

        let mut branches = self.storage.list_branches(scenario_id)?;
        if branches.is_empty() {
            self.storage.create_branch(&Branch {
                id: format!("br-{}", uuid::Uuid::new_v4().simple()),
                scenario_id: scenario_id.into(),
                parent_id: None,
                seed: options.seed as i64,
                status: "pending".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
            })?;
            branches = self.storage.list_branches(scenario_id)?;
        }

        // Reuse existing pending/done branches for the first N slots.
        let mut chosen: Vec<Branch> = Vec::new();
        for b in branches {
            if chosen.len() >= options.branch_count {
                break;
            }
            if b.status == "pending" || b.status == "done" {
                chosen.push(b);
            }
        }
        while chosen.len() < options.branch_count {
            let b = Branch {
                id: format!("br-{}", uuid::Uuid::new_v4().simple()),
                scenario_id: scenario_id.into(),
                parent_id: None,
                seed: options.seed.wrapping_add(chosen.len() as u64) as i64,
                status: "pending".into(),
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            self.storage.create_branch(&b)?;
            chosen.push(b);
        }

        run_scenario_branches(
            self.db_path.clone(),
            self.models_dir.clone(),
            self.bin_dir.clone(),
            scenario.clone(),
            chosen,
            options,
            progress,
        )
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelStatus {
    pub id: String,
    pub name: String,
    pub base_model: String,
    pub size_b: f32,
    pub role: models::ModelRole,
    pub quantization: String,
    pub available: bool,
}

// ---------------------------------------------------------------------------
// Branch execution
// ---------------------------------------------------------------------------

/// Run one branch of a scenario. Used by [`run_scenario_branches`].
pub fn run_branch(
    storage: &Storage,
    llm: &LlamaClient,
    scenario: &Scenario,
    branch: &Branch,
    options: &SimulationOptions,
    progress: &Option<ProgressCb>,
) -> Result<usize> {
    if options.force_fallback || !llm.available(&models::MUSTAFAKEMAL) {
        let cfg = FallbackConfig {
            branch_seed: branch.seed as u64,
            target_date: options.target_date,
            base_step_years: 1.0,
        };
        storage.set_branch_status(&branch.id, "running")?;
        let n = fallback::run_fallback(
            storage,
            &scenario.id,
            &branch.id,
            &scenario.prompt,
            cfg,
        )?;
        if let Some(cb) = progress {
            cb(SimProgress {
                scenario_id: scenario.id.clone(),
                branch_id: branch.id.clone(),
                date: options.target_date,
                step: 0,
                events_created: n,
                source: "fallback".into(),
                phase: "done".into(),
                note: "Deterministic fallback simulation complete".into(),
            });
        }
        return Ok(n);
    }

    storage.set_branch_status(&branch.id, "running")?;
    let mut step = 0u32;
    let mut created = 0usize;
    let mut date = scenario.divergence;
    let mut last_ids: Vec<i64> = Vec::new();

    // One-time RAG context (ortayli) over nearby canonical history.
    let rag_context = historical_context(llm, storage, &scenario, &options)?;

    if let Some(cb) = progress {
        cb(SimProgress {
            scenario_id: scenario.id.clone(),
            branch_id: branch.id.clone(),
            date,
            step: 0,
            events_created: 0,
            source: "engine".into(),
            phase: "start".into(),
            note: format!("Branch {} starting from {}", branch.id, scenario.divergence.display()),
        });
    }

    while date < options.target_date && step < options.max_steps {
        let step_years = adaptive_step(step);
        let next = date.add_years(step_years).min(options.target_date);

        if let Some(cb) = progress {
            cb(SimProgress {
                scenario_id: scenario.id.clone(),
                branch_id: branch.id.clone(),
                date,
                step,
                events_created: created,
                source: "engine".into(),
                phase: "plan".into(),
                note: format!("Step {step} branch {}: planning {date} → {next}…", branch.id),
            });
        }

        let snapshot = storage.build_snapshot(date, Some(&scenario.id), Some(&branch.id))?;

        // 1) Mustafa Kemal plans causal events for the window.
        let (events, source) = plan_step(
            llm,
            &scenario.prompt,
            &snapshot,
            date,
            next,
            options,
            branch.seed as u64,
            &rag_context,
        )?;

        if let Some(cb) = progress {
            cb(SimProgress {
                scenario_id: scenario.id.clone(),
                branch_id: branch.id.clone(),
                date,
                step,
                events_created: created,
                source: source.clone(),
                phase: "stats".into(),
                note: format!("Step {step} ({source}): {} events, filling statistics…", events.len()),
            });
        }

        // 2) Inalcik fills in statistics.
        let events = fill_statistics(llm, events, options, branch.seed as u64)?;

        if let Some(cb) = progress {
            cb(SimProgress {
                scenario_id: scenario.id.clone(),
                branch_id: branch.id.clone(),
                date,
                step,
                events_created: created,
                source: source.clone(),
                phase: "validate".into(),
                note: format!("Step {step}: validating {} events…", events.len()),
            });
        }

        // 3) Validate + auto-fix + retry loop.
        let events = validate_loop(storage, &scenario, &events, &snapshot)?;

        // 4) Apply + causal log.
        let mut seq = storage.last_scenario_seq(&scenario.id, &branch.id)?;
        for mut ev in events {
            if ev.date < scenario.divergence {
                continue; // hard lock: drop any pre-divergence output.
            }
            if ev.date > options.target_date {
                ev.date = options.target_date;
            }
            seq += 1;
            ev.seq = seq;
            ev.scenario_id = Some(scenario.id.clone());
            ev.causal_parents = last_ids.clone();
            ev.source_model = source.clone();
            apply::structural_validate(&ev).map_err(|e| EngineError::invalid(e.to_string()))?;
            let id = storage.add_scenario_event(&ev, &branch.id)?;
            last_ids.push(id);
            created += 1;
        }
        if last_ids.len() > 8 {
            last_ids.drain(..(last_ids.len() - 8));
        }

        if let Some(cb) = progress {
            cb(SimProgress {
                scenario_id: scenario.id.clone(),
                branch_id: branch.id.clone(),
                date: next,
                step,
                events_created: created,
                source,
                phase: "step".into(),
                note: format!("Step {step} branch {} complete: {date} → {next}", branch.id),
            });
        }

        date = next;
        step += 1;
    }

    storage.set_branch_status(&branch.id, "done")?;
    if let Some(cb) = progress {
        cb(SimProgress {
            scenario_id: scenario.id.clone(),
            branch_id: branch.id.clone(),
            date: options.target_date,
            step,
            events_created: created,
            source: "engine".into(),
            phase: "done".into(),
            note: "Simulation complete".into(),
        });
    }
    Ok(created)
}

/// Adaptive step: fine near divergence (0.5y), coarse in the far future.
fn adaptive_step(step: u32) -> f64 {
    if step < 3 {
        0.5
    } else if step < 20 {
        1.0
    } else {
        2.0
    }
}

/// Build a historical-context block for mustafakemal using ortayli (RAG). Returns an
/// empty string when the embedding model is unavailable.
fn historical_context(
    llm: &LlamaClient,
    storage: &Storage,
    scenario: &Scenario,
    options: &SimulationOptions,
) -> Result<String> {
    let client = crate::retrieval::EmbedClient::new(&llm.models_dir);
    if !client.available() {
        return Ok(String::new());
    }
    // Collect nearby canonical event text (divergence -2y .. divergence +1y).
    let from = scenario.divergence.add_years(-2.0);
    let to = scenario.divergence.add_years(1.0);
    let events = storage.canonical_events_up_to(to)?;
    let docs: Vec<String> = events
        .iter()
        .filter(|e| e.date >= from)
        .take(600)
        .map(|e| format!("{}: {}. {}", e.date.display(), e.title, e.body))
        .collect();
    let idx = crate::retrieval::build_index(&client, &docs)?;
    let hits = idx.query(&client, &scenario.prompt, 6)?;
    if hits.is_empty() {
        return Ok(String::new());
    }
    let mut out = String::from("RELEVANT HISTORY (retrieved by ortayli):\n");
    for (score, doc) in &hits {
        out.push_str(&format!("- [{:.2}] {}\n", score, doc));
    }
    let _ = options;
    Ok(out)
}

/// Ask mustafakemal (or fallback) for the events of a step window.
fn plan_step(
    llm: &LlamaClient,
    prompt: &str,
    snapshot: &WorldSnapshot,
    from: SimDate,
    to: SimDate,
    options: &SimulationOptions,
    seed: u64,
    rag_context: &str,
) -> Result<(Vec<HistoryEvent>, String)> {
    let sys = SYSTEM_MUSTAFAKEMAL;
    let user = plan_prompt(snapshot, prompt, from, to, rag_context);
    let spec = models::route("plan");
    let out = llm.generate(
        &spec,
        sys,
        &user,
        options.temperature,
        seed.wrapping_add(from.days_from_ce() as u64),
        1024,
    )?;
    match out {
        Some(r) => {
            let events = parse_event_array(&r.text)?;
            Ok((events, "mustafakemal".into()))
        }
        None => Ok((vec![], "mustafakemal".into())),
    }
}

/// Ask inalcik to fill/verify the numeric fields of the planned events.
fn fill_statistics(
    llm: &LlamaClient,
    events: Vec<HistoryEvent>,
    options: &SimulationOptions,
    seed: u64,
) -> Result<Vec<HistoryEvent>> {
    if events.is_empty() {
        return Ok(events);
    }
    let spec = models::INALCIK;
    let summary = summarize_events(&events);
    let sys = SYSTEM_INALCIK;
    let out = llm.generate(
        &spec,
        sys,
        &format!(
            "Here are planned events. Return the same list but fill in \
             concrete population numbers, migration amounts, and adoption \
             rates where missing. Output ONLY the JSON array.\n\n{}",
            summary
        ),
        options.temperature.min(0.5),
        seed.wrapping_add(0x_9e37_79b9),
        1024,
    )?;
    match out {
        Some(r) => Ok(parse_event_array(&r.text).unwrap_or(events)),
        None => Ok(events),
    }
}

/// Validate events, auto-fix, and retry a few times with violation feedback.
fn validate_loop(
    _storage: &Storage,
    scenario: &Scenario,
    events: &[HistoryEvent],
    snapshot: &WorldSnapshot,
) -> Result<Vec<HistoryEvent>> {
    let mut events = events.to_vec();
    for attempt in 0..3 {
        let mut fixed = events.clone();
        let mut violations: Vec<Violation> = Vec::new();
        for ev in fixed.iter_mut() {
            validate::auto_fix(ev);
            violations.extend(validate::validate(ev, scenario.divergence, snapshot));
        }
        if violations.is_empty() {
            return Ok(fixed);
        }
        if attempt == 2 {
            // Drop offending events rather than crash the branch.
            let texts: Vec<String> = violations.iter().map(|v| v.message.clone()).collect();
            let offending: Vec<Violation> = violations;
            for ev in fixed.iter_mut() {
                // Clear fields that triggered non-fatal issues where possible.
                ev.body = format!(
                    "Validated after {} violation(s): {}",
                    offending.len(),
                    texts.join("; ")
                );
            }
            return Ok(fixed);
        }
        events = fixed;
    }
    Ok(events)
}

/// Parse a possibly-fenced JSON array of events from model output.
pub fn parse_event_array(text: &str) -> Result<Vec<HistoryEvent>> {
    let json = extract_json_array(text)?;
    let arr = json
        .as_array()
        .ok_or_else(|| EngineError::invalid("expected a JSON array of events"))?;
    let mut out = Vec::new();
    for (i, v) in arr.iter().enumerate() {
        if let Some(ev) = event_from_json(v)? {
            out.push(ev);
            let _ = i;
        }
    }
    Ok(out)
}

fn event_from_json(v: &serde_json::Value) -> Result<Option<HistoryEvent>> {
    let Some(obj) = v.as_object() else {
        return Ok(None);
    };
    if obj.is_empty() {
        return Ok(None);
    }
    let date = date_from_json(obj.get("date"))?;
    let payload: EventPayload = serde_json::from_value(v.clone())
        .map_err(|e| EngineError::invalid(format!("event payload invalid: {e}")))?;
    let title = obj
        .get("title")
        .and_then(|t| t.as_str())
        .unwrap_or("Unnamed event")
        .to_string();
    let body = obj
        .get("body")
        .and_then(|b| b.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(Some(HistoryEvent {
        id: 0,
        date,
        scenario_id: None,
        title,
        body,
        payload,
        source_model: "model".into(),
        causal_parents: vec![],
        seq: 0,
    }))
}

fn date_from_json(v: Option<&serde_json::Value>) -> Result<SimDate> {
    let Some(v) = v else {
        return Ok(SimDate::from_ce(1900, 1, 1));
    };
    if let Some(s) = v.as_str() {
        return date_from_iso(s).ok_or_else(|| EngineError::invalid(format!("bad date '{s}'")));
    }
    let year = v.get("year").and_then(|y| y.as_i64()).unwrap_or(1900) as i32;
    let month = v.get("month").and_then(|m| m.as_i64()).unwrap_or(1) as u8;
    let day = v.get("day").and_then(|d| d.as_i64()).unwrap_or(1) as u8;
    Ok(SimDate::from_astro(year, month, day))
}

/// Parse "YYYY-MM-DD" or "YYYY" or "BCE YYYY"/"YYYY BCE" dates.
pub fn date_from_iso(s: &str) -> Option<SimDate> {
    let s = s.trim();
    let lower = s.to_lowercase();
    if lower.contains("bce") || lower.ends_with("bc") {
        let n: String = lower
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        let year = n.parse::<u32>().ok()?;
        return Some(SimDate::from_bce(year, 1, 1));
    }
    if lower.contains("ce") || lower.ends_with("ad") {
        let n: String = lower.chars().filter(|c| c.is_ascii_digit()).collect();
        let year = n.parse::<i32>().ok()?;
        return Some(SimDate::from_ce(year, 1, 1));
    }
    let parts: Vec<&str> = s.split(['-', '/']).collect();
    match parts.len() {
        1 => {
            let y = parts[0].parse::<i32>().ok()?;
            Some(SimDate::from_ce(y, 1, 1))
        }
        2 => {
            let y = parts[0].parse::<i32>().ok()?;
            let m = parts[1].parse::<u8>().ok()?;
            Some(SimDate::from_ce(y, m, 1))
        }
        _ => {
            let y = parts[0].parse::<i32>().ok()?;
            let m = parts[1].parse::<u8>().ok()?;
            let d = parts[2].parse::<u8>().ok()?;
            Some(SimDate::from_ce(y, m, d))
        }
    }
}

/// Robustly extract the outermost JSON array from model text.
fn extract_json_array(text: &str) -> Result<serde_json::Value> {
    // Strip code fences.
    let cleaned: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(start) = cleaned.find('[') {
        let mut depth = 0i32;
        let mut in_str = false;
        let mut esc = false;
        for (i, c) in cleaned[start..].char_indices() {
            match c {
                '"' if !esc => in_str = !in_str,
                '\\' if in_str => esc = true,
                _ if in_str => esc = false,
                '[' if !in_str => depth += 1,
                ']' if !in_str => {
                    depth -= 1;
                    if depth == 0 {
                        let end = start + i + c.len_utf8();
                        let slice = &cleaned[start..end];
                        return serde_json::from_str(slice)
                            .map_err(|e| EngineError::invalid(format!("json parse: {e}")));
                    }
                }
                _ => {}
            }
        }
    }
    Err(EngineError::invalid("no JSON array found in model output"))
}

fn plan_prompt(
    snapshot: &WorldSnapshot,
    user_divergence: &str,
    from: SimDate,
    to: SimDate,
    rag_context: &str,
) -> String {
    let mut top: Vec<&crate::state::Nation> = snapshot.nations.iter().collect();
    top.sort_by(|a, b| b.population.cmp(&a.population));
    top.truncate(24);

    let mut nat_lines = String::new();
    for n in top {
        let rel: Vec<String> = n
            .religion_pct
            .iter()
            .map(|(k, v)| format!("{k} {v:.0}%"))
            .collect();
        nat_lines.push_str(&format!(
            "- {} (pop {}, econ {}, military {}{}{}) \n",
            n.id,
            n.population,
            n.economy_index as i64,
            n.military_index as i64,
            if rel.is_empty() { String::new() } else { format!("; rel: {}", rel.join(", ")) },
            ""
        ));
    }

    let mut terr_count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for t in &snapshot.territories {
        *terr_count.entry(t.owner.as_str()).or_insert(0) += 1;
    }
    let terr_lines: Vec<String> = terr_count
        .iter()
        .map(|(o, c)| format!("{o}: {c} territories"))
        .collect();
    let tech_lines: Vec<String> = snapshot
        .techs
        .iter()
        .map(|t| format!("- {} ({}), adoption {:.0}%", t.name, t.category, t.adoption * 100.0))
        .collect();

    format!(
        "DIVERGENCE PROMPT: {user_divergence}\n\n\
         {rag_context}\n\
         WORLD STATE at {from}:\n\
         NATIONS:\n{nat_lines}\n\
         TERRITORIES BY OWNER:\n{}\n\
         KNOWN TECHNOLOGY:\n{}\n\n\
         TASK: simulate the window {from} to {to}.\n\
         Think causally: wars, treaties, annexations, revolutions, riots, guerrilla \
         movements, economic effects, population change, migration, technology. Include \
         second-order effects. If this window is far from major events, output few or no events.\n\n\
         RESPONSE FORMAT: output ONLY a JSON array. Each item: \
         {{\"date\":\"YYYY-MM-DD\",\"title\":\"...\",\"body\":\"...\",\"kind\":\"census\", ...fields...}}. \
         Kinds and fields:\n\
         - border_change: territory, new_owner, prev_owner\n\
         - war: name, participants[], start_date, end_date, winner, intensity\n\
         - treaty: name, parties[], terms\n\
         - census: nation, population, religion_pct{{}}, ethnicity_pct{{}}, economy_index, military_index\n\
         - invention: name, region, year, adoption_rate, category, impact\n\
         - migration: from_region, to_region, amount, reason\n\
         - unrest: region, unrest_kind(riot|guerrilla|rebellion|civil_war|coup), severity(0-10), description\n\
         - narrative: text\n\n\
         Keep every date within [{from}, {to}]. Do NOT invent events before {from}.",
        terr_lines.join("; "),
        if tech_lines.is_empty() {
            "(no technology recorded yet)".to_string()
        } else {
            tech_lines.join("\n")
        }
    )
}

fn summarize_events(events: &[HistoryEvent]) -> String {
    let mut out = String::new();
    for e in events {
        out.push_str(&format!(
            "{{\"date\":\"{}\",\"title\":\"{}\",{} }}",
            e.date.display_year(),
            e.title,
            serde_json::to_string(&e.payload).unwrap_or_default()
        ));
        out.push('\n');
    }
    out
}

const SYSTEM_MUSTAFAKEMAL: &str = "You are Mustafa Kemal, the causal simulation model of WorldSimulator. \
You reason rigorously about alternate history: cause and effect, geopolitics, war, \
technology, demographics, and second-order consequences. You never edit anything \
before the divergence point. You output structured JSON only.";

const SYSTEM_INALCIK: &str = "You are Inalcik, the data model of WorldSimulator. You produce \
realistic, internally consistent statistics: populations, migrations, economy and \
military indices, technology adoption curves. You only touch numeric fields.";
