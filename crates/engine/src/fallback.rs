//! Deterministic rule-based fallback simulator.
//!
//! Used when the GGUF models are unavailable. It parses the scenario prompt,
//! identifies the protagonist, and produces a plausible causal timeline
//! (wars, annexations, unrest, censuses, shifted inventions) using a seeded
//! RNG. Every event passes the same validation pipeline as model output.

use crate::events::{
    BorderChange, Census, EventPayload, HistoryEvent, Invention, Treaty, Unrest, War,
};
use crate::state::WorldSnapshot;
use crate::storage::Storage;
use crate::{Result, SimDate};
use rand::Rng;
use std::collections::HashMap;

/// Known nations for prompt analysis.
pub fn known_nations() -> Vec<(String, String)> {
    vec![
        ("USA".into(), "United States".into()),
        ("GER".into(), "Germany".into()),
        ("USSR".into(), "Soviet Union".into()),
        ("GBR".into(), "United Kingdom".into()),
        ("FRA".into(), "France".into()),
        ("JPN".into(), "Japan".into()),
        ("CHN".into(), "China".into()),
        ("IRN".into(), "Iran".into()),
        ("ITA".into(), "Italy".into()),
        ("IND".into(), "India".into()),
        ("ISR".into(), "Israel".into()),
        ("TUR".into(), "Turkey".into()),
    ]
}

/// Result of parsing a scenario prompt.
#[derive(Debug, Clone)]
pub struct PromptIntent {
    pub combatants: Vec<String>,
    pub winner: Option<String>,
    pub theme: String,
}

/// Analyze a free-text prompt.
pub fn analyze_prompt(prompt: &str) -> PromptIntent {
    let lower = prompt.to_lowercase();
    let mut combatants: Vec<String> = Vec::new();
    for (id, name) in known_nations() {
        if lower.contains(&name.to_lowercase()) || lower.contains(&id.to_lowercase()) {
            combatants.push(id);
        }
    }

    let winner: Option<String>;
    let theme;
    if lower.contains("nazi") || (lower.contains("germany") && lower.contains("win")) {
        theme = "nazi_victory".into();
        if !combatants.contains(&"GER".to_string()) {
            combatants.push("GER".into());
        }
        winner = Some("GER".into());
        for extra in ["FRA", "GBR", "USSR", "USA"] {
            if !combatants.contains(&extra.to_string()) {
                combatants.push(extra.to_string());
            }
        }
    } else if lower.contains("conquer") && lower.contains("iran") {
        theme = "conquest".into();
        if !combatants.contains(&"USA".to_string()) {
            combatants.push("USA".into());
        }
        if !combatants.contains(&"IRN".to_string()) {
            combatants.push("IRN".into());
        }
        winner = Some("USA".into());
    } else if lower.contains("conquer") {
        theme = "conquest".into();
        winner = combatants.first().cloned();
    } else if lower.contains("collapse") || lower.contains("fall") {
        theme = "collapse".into();
        winner = None;
    } else {
        theme = "generic".into();
        if combatants.len() < 2 {
            combatants.push("USA".into());
            combatants.push("CHN".into());
        }
        winner = Some(combatants[0].clone());
    }
    PromptIntent { combatants, winner, theme }
}

/// Configuration for a fallback run.
#[derive(Debug, Clone)]
pub struct FallbackConfig {
    pub branch_seed: u64,
    pub target_date: SimDate,
    /// Step size in years; adaptive multipliers applied near the divergence.
    pub base_step_years: f64,
}

impl Default for FallbackConfig {
    fn default() -> Self {
        FallbackConfig {
            branch_seed: 42,
            target_date: SimDate::from_ce(2100, 1, 1),
            base_step_years: 1.0,
        }
    }
}

pub struct FallbackSim<'a> {
    storage: &'a Storage,
    scenario_id: &'a str,
    branch_id: &'a str,
    cfg: FallbackConfig,
    intent: PromptIntent,
    rng: rand::rngs::StdRng,
    seq: i64,
    last_id: Option<i64>,
}

impl<'a> FallbackSim<'a> {
    pub fn new(
        storage: &'a Storage,
        scenario_id: &'a str,
        branch_id: &'a str,
        intent: PromptIntent,
        cfg: FallbackConfig,
    ) -> Self {
        let rng = rand::SeedableRng::seed_from_u64(cfg.branch_seed);
        let seq = storage.last_scenario_seq(scenario_id, branch_id).unwrap_or(0);
        FallbackSim {
            storage,
            scenario_id,
            branch_id,
            cfg,
            intent,
            rng,
            seq,
            last_id: None,
        }
    }

    fn emit(
        &mut self,
        date: SimDate,
        title: impl Into<String>,
        body: impl Into<String>,
        payload: EventPayload,
    ) -> Result<i64> {
        self.seq += 1;
        let parents: Vec<i64> = self.last_id.into_iter().collect();
        let ev = HistoryEvent {
            id: 0,
            date,
            scenario_id: Some(self.scenario_id.to_string()),
            title: title.into(),
            body: body.into(),
            payload,
            source_model: "fallback".into(),
            causal_parents: parents,
            seq: self.seq,
        };
        let id = self.storage.add_scenario_event(&ev, self.branch_id)?;
        self.last_id = Some(id);
        Ok(id)
    }

    /// Run the fallback simulation from the scenario divergence to the target.
    pub fn run(&mut self) -> Result<usize> {
        let scenario = self
            .storage
            .get_scenario(self.scenario_id)?
            .ok_or_else(|| crate::EngineError::ScenarioNotFound(self.scenario_id.into()))?;
        let divergence = scenario.divergence;

        let mut snap = self.storage.build_snapshot(divergence, Some(self.scenario_id), Some(self.branch_id))?;
        let losers: Vec<String> = match &self.intent.winner {
            Some(w) => self
                .intent
                .combatants
                .iter()
                .filter(|c| *c != w)
                .cloned()
                .collect(),
            None => self.intent.combatants.clone(),
        };

        let mut date = divergence;
        let mut step = 0u32;
        let mut created = 0;

        while date < self.cfg.target_date {
            let step_years = self.step_size(step);
            let next = date.add_years(step_years).min(self.cfg.target_date);

            // War start (step 0).
            if step == 0 && !losers.is_empty() {
                let war = War {
                    name: format!(
                        "{} War",
                        self.intent
                            .winner
                            .clone()
                            .unwrap_or_else(|| "Continental".into())
                    ),
                    participants: self.intent.combatants.clone(),
                    start_date: divergence,
                    end_date: None,
                    winner: None,
                    intensity: 8,
                    caused_by: vec![],
                };
                self.emit(
                    divergence,
                    format!("War begins: {}", war.name),
                    "Hostilities begin at the point of divergence.".to_string(),
                    EventPayload::War(war),
                )?;
                created += 1;
            }

            // Territorial transfers from losers to winner.
            let winner = self.intent.winner.clone();
            if let Some(w) = &winner {
                let conquered = self.conquer_step(&mut snap, w, &losers, next);
                created += conquered;
            } else {
                // Collapse: losers fragment into unrest.
                for l in &losers {
                    self.unrest_step(&snap, l, next);
                }
            }

            // Unrest in recently annexed territories.
            if let Some(w) = &winner {
                let annexed = snap
                    .nations
                    .iter()
                    .find(|n| &n.id == w)
                    .map(|n| n.territories.len())
                    .unwrap_or(0);
                if self.rng.gen_ratio(2, 5) && annexed > 0 {
                    let severity = (2 + self.rng.gen_range(0..6)) as f64;
                    self.emit(
                        next,
                        format!("Guerrilla resistance in annexed territories of {w}"),
                        "Occupied populations resist the new administration.".to_string(),
                        EventPayload::Unrest(Unrest {
                            region: w.clone(),
                            unrest_kind: "guerrilla".into(),
                            severity,
                            description: "Second-order effect of occupation: local insurgency.".into(),
                            caused_by: vec![],
                        }),
                    )?;
                    created += 1;
                }
            }

            // Census / demographic update.
            created += self.census_step(&snap, next);

            // Possible invention shift.
            if self.rng.gen_ratio(1, 6) {
                created += self.invention_step(&snap, next);
            }

            date = next;
            step += 1;
            if step > 300 {
                break;
            }
        }

        // Final peace treaty.
        if !losers.is_empty() {
            let winner = self.intent.winner.clone().unwrap_or_default();
            self.end_war(self.cfg.target_date, &winner);
        }

        self.storage
            .set_branch_status(self.branch_id, "done")?;
        Ok(created)
    }

    fn step_size(&self, step: u32) -> f64 {
        // Fine-grained near divergence, coarse later.
        let base = self.cfg.base_step_years;
        if step < 3 {
            base * 0.5
        } else if step < 20 {
            base
        } else {
            base * 2.0
        }
    }

    fn conquer_step(
        &mut self,
        snap: &mut WorldSnapshot,
        winner: &str,
        losers: &[String],
        date: SimDate,
    ) -> usize {
        let mut count = 0;
        let candidate: Vec<(String, String)> = snap
            .territories
            .iter()
            .filter(|t| losers.contains(&t.owner))
            .map(|t| (t.id.clone(), t.owner.clone()))
            .collect();
        // Take up to a fraction each step.
        let take = ((candidate.len() as f64) * 0.2).ceil() as usize;
        let mut taken = 0;
        for (tid, prev) in candidate {
            if taken >= take.max(1) {
                break;
            }
            let _ = self.emit(
                date,
                format!("Annexation of {tid} by {winner}"),
                format!("Territory {tid} falls under {winner} control."),
                EventPayload::BorderChange(BorderChange {
                    territory: tid.clone(),
                    new_owner: winner.into(),
                    prev_owner: prev,
                    geometry_geojson: None,
                    caused_by: vec![],
                }),
            );
            if let Some(t) = snap.territories.iter_mut().find(|t| t.id == tid) {
                t.owner = winner.to_string();
            }
            if let Some(n) = snap.nations.iter_mut().find(|n| n.id == winner) {
                n.territories.push(tid);
            }
            count += 1;
            taken += 1;
        }
        count
    }

    fn end_war(&mut self, date: SimDate, winner: &str) {
        let _ = self.emit(
            date,
            "War ends",
            format!("The conflict concludes with {winner} victorious."),
            EventPayload::War(War {
                name: "War".into(),
                participants: self.intent.combatants.clone(),
                start_date: date,
                end_date: Some(date),
                winner: Some(winner.into()),
                intensity: 8,
                caused_by: vec![],
            }),
        );
        let _ = self.emit(
            date,
            "Peace treaty",
            format!("A treaty formalizes {winner} gains."),
            EventPayload::Treaty(Treaty {
                name: format!("{winner} Settlement"),
                parties: self.intent.combatants.clone(),
                transfers: vec![],
                terms: format!("{winner} assumes control over the defeated territories."),
                caused_by: vec![],
            }),
        );
    }

    fn unrest_step(&mut self, snap: &WorldSnapshot, nation: &str, date: SimDate) {
        if self.rng.gen_ratio(1, 3) {
            let kinds = ["riot", "rebellion", "civil_war", "coup"];
            let kind = kinds[self.rng.gen_range(0..kinds.len())];
            let severity = (5 + self.rng.gen_range(0..5)) as f64;
            let _ = self.emit(
                date,
                format!("{kind} in {nation}"),
                "The collapse triggers internal conflict.".to_string(),
                EventPayload::Unrest(Unrest {
                    region: nation.into(),
                    unrest_kind: kind.into(),
                    severity,
                    description: "State authority dissolves; factions compete for control.".into(),
                    caused_by: vec![],
                }),
            );
            let _ = snap;
        }
    }

    fn census_step(&mut self, snap: &WorldSnapshot, date: SimDate) -> usize {
        let mut count = 0;
        for n in snap.nations.iter().take(6) {
            if self.rng.gen_ratio(1, 2) {
                let delta = (n.population as f64 * self.rng.gen_range(-0.02..0.03)) as i64;
                let pop = (n.population + delta).max(1000);
                let mut religion = HashMap::new();
                let mut ethnicity = HashMap::new();
                if n.religion_pct.is_empty() {
                    religion.insert("Majority".into(), 60.0);
                    religion.insert("Minorities".into(), 40.0);
                } else {
                    for (k, v) in &n.religion_pct {
                        religion.insert(k.clone(), *v);
                    }
                }
                if n.ethnicity_pct.is_empty() {
                    ethnicity.insert("Majority".into(), 80.0);
                    ethnicity.insert("Minorities".into(), 20.0);
                } else {
                    for (k, v) in &n.ethnicity_pct {
                        ethnicity.insert(k.clone(), *v);
                    }
                }
                let _ = self.emit(
                    date,
                    format!("Census of {}", n.id),
                    format!("Population reaches {pop}."),
                    EventPayload::Census(Census {
                        nation: n.id.clone(),
                        population: pop,
                        religion_pct: religion,
                        ethnicity_pct: ethnicity,
                        economy_index: (n.economy_index * 1.02).min(100.0),
                        military_index: (n.military_index * 1.01).min(100.0),
                        caused_by: vec![],
                    }),
                );
                count += 1;
            }
        }
        count
    }

    fn invention_step(&mut self, snap: &WorldSnapshot, date: SimDate) -> usize {
        let region = self
            .intent
            .winner
            .clone()
            .unwrap_or_else(|| "USA".into());
        let pool = [
            ("Orbital colonization program", "space", 0.6),
            ("Autonomous war fleet", "military", 0.7),
            ("Cold fusion pilot plant", "energy", 0.4),
            ("Universal vaccine platform", "medical", 0.5),
            ("Quantum mainframe", "computing", 0.4),
            ("Intercontinental maglev network", "transport", 0.6),
        ];
        let (name, cat, adopt) = pool[self.rng.gen_range(0..pool.len())];
        let _ = self.emit(
            date,
            format!("Invention: {name}"),
            format!("{region} achieves a technological breakthrough ahead of the real timeline."),
            EventPayload::Invention(Invention {
                name: name.into(),
                tech_id: format!("alt-{}-{}", cat, date.year),
                region: region.clone(),
                year: date.year,
                adoption_rate: adopt,
                category: cat.into(),
                impact: 1.5,
                caused_by: vec![],
            }),
        );
        let _ = snap;
        1
    }
}

/// Convenience entry point: run a full fallback simulation.
pub fn run_fallback(
    storage: &Storage,
    scenario_id: &str,
    branch_id: &str,
    prompt: &str,
    config: FallbackConfig,
) -> Result<usize> {
    let intent = analyze_prompt(prompt);
    let mut sim = FallbackSim::new(storage, scenario_id, branch_id, intent, config);
    sim.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[test]
    fn prompt_analysis_finds_nazi_theme() {
        let i = analyze_prompt("What if the Nazis won in 1943?");
        assert_eq!(i.theme, "nazi_victory");
        assert_eq!(i.winner.as_deref(), Some("GER"));
    }

    #[test]
    fn fallback_produces_events_after_divergence() {
        let s = Storage::open_in_memory().unwrap();
        s.create_scenario("s1", "test", "What if the Nazis won in 1943?", SimDate::from_ce(1943, 6, 1))
            .unwrap();
        s.create_branch(&crate::storage::Branch {
            id: "b1".into(),
            scenario_id: "s1".into(),
            parent_id: None,
            seed: 1,
            status: "running".into(),
            created_at: "now".into(),
        })
        .unwrap();
        let n = run_fallback(
            &s,
            "s1",
            "b1",
            "What if the Nazis won in 1943?",
            FallbackConfig {
                target_date: SimDate::from_ce(1960, 1, 1),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(n > 0);
        let events = s
            .scenario_events_up_to("s1", "b1", SimDate::from_ce(9999, 12, 28))
            .unwrap();
        assert!(events.iter().all(|e| e.date >= SimDate::from_ce(1943, 6, 1)));
        assert!(events.iter().all(|e| e.source_model == "fallback"));
    }
}
