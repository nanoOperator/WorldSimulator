//! Applying history events to a [`WorldSnapshot`]. The canonical and scenario
//! timelines are replayed through this function to materialize world state.

use crate::events::{EventPayload, HistoryEvent};
use crate::state::{Nation, TechState, WorldSnapshot};
use crate::EngineError;

/// Apply a single history event to a snapshot (in date order).
pub fn apply(snap: &mut WorldSnapshot, ev: &HistoryEvent) {
    match &ev.payload {
        EventPayload::EpochBaseline(b) => {
            snap.nations = b.nations.clone();
            snap.territories = b.territories.clone();
            // Techs accumulate across baselines: a later baseline can redefine
            // an existing tech (e.g. raising its adoption), but previously
            // invented technology persists into later eras.
            let mut techs: std::collections::HashMap<String, TechState> = Default::default();
            for t in snap.techs.drain(..) {
                techs.insert(t.tech_id.clone(), t);
            }
            for t in &b.techs {
                techs.insert(t.tech_id.clone(), t.clone());
            }
            snap.techs = techs.into_values().collect();
            // Recompute owned-territory lists for every nation.
            let mut owned: std::collections::HashMap<&str, Vec<String>> = Default::default();
            for t in &snap.territories {
                owned.entry(t.owner.as_str()).or_default().push(t.id.clone());
            }
            for n in snap.nations.iter_mut() {
                n.territories = owned.get(n.id.as_str()).cloned().unwrap_or_default();
            }
        }
        EventPayload::BorderChange(c) => {
            if let Some(t) = snap.territories.iter_mut().find(|t| t.id == c.territory) {
                t.owner = c.new_owner.clone();
            }
            if let Some(g) = &c.geometry_geojson {
                if let Some(t) = snap.territories.iter_mut().find(|t| t.id == c.territory) {
                    t.geometry_geojson = g.clone();
                }
            }
        }
        EventPayload::NationFounded(n) => {
            if !snap.nations.iter().any(|x| x.id == n.id) {
                let color = n.color.clone().unwrap_or_else(|| crate::state::color_for_nation(&n.id));
                snap.nations.push(Nation {
                    id: n.id.clone(),
                    name: n.name.clone(),
                    color,
                    population: 0,
                    religion_pct: vec![],
                    ethnicity_pct: vec![],
                    economy_index: 0.0,
                    military_index: 0.0,
                    territories: vec![],
                });
            }
        }
        EventPayload::NationCollapsed(c) => {
            if let Some(pos) = snap.nations.iter().position(|n| n.id == c.nation) {
                snap.nations.remove(pos);
            }
            let successor = c.successor.clone();
            for t in snap.territories.iter_mut() {
                if t.owner == c.nation {
                    t.owner = successor.clone().unwrap_or_else(|| "_unclaimed_".into());
                }
            }
        }
        EventPayload::Census(c) => {
            let color = c
                .nation
                .get(..)
                .map(crate::state::color_for_nation);
            if !snap.nations.iter().any(|n| n.id == c.nation) {
                snap.nations.push(Nation {
                    id: c.nation.clone(),
                    name: c.nation.clone(),
                    color: color.unwrap_or_else(|| "#888888".into()),
                    population: 0,
                    religion_pct: vec![],
                    ethnicity_pct: vec![],
                    economy_index: 0.0,
                    military_index: 0.0,
                    territories: vec![],
                });
            }
            if let Some(n) = snap.nations.iter_mut().find(|n| n.id == c.nation) {
                n.population = c.population;
                n.religion_pct = c
                    .religion_pct
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                n.ethnicity_pct = c
                    .ethnicity_pct
                    .iter()
                    .map(|(k, v)| (k.clone(), *v))
                    .collect();
                n.economy_index = c.economy_index;
                n.military_index = c.military_index;
            }
        }
        EventPayload::Migration(m) => {
            let from_pop = snap
                .nations
                .iter()
                .find(|n| n.id == m.from_region)
                .map(|n| n.population);
            let to_index = snap.nations.iter().position(|n| n.id == m.to_region);
            if let Some(f) = from_pop {
                if let Some(to_pos) = to_index {
                    if f >= m.amount {
                        if let Some(n) = snap.nations.iter_mut().find(|n| n.id == m.from_region) {
                            n.population -= m.amount;
                        }
                        snap.nations[to_pos].population += m.amount;
                    } else {
                        let from_pos = snap.nations.iter().position(|n| n.id == m.from_region);
                        if let Some(fp) = from_pos {
                            let moving = snap.nations[fp].population;
                            snap.nations[to_pos].population += moving;
                            snap.nations[fp].population = 0;
                        }
                    }
                }
            }
        }
        EventPayload::Invention(i) => {
            let existing = snap
                .techs
                .iter_mut()
                .find(|t| t.tech_id == i.tech_id);
            if let Some(t) = existing {
                t.name = i.name.clone();
                t.adoption = (t.adoption + i.adoption_rate).min(1.0);
            } else {
                snap.techs.push(TechState {
                    tech_id: i.tech_id.clone(),
                    name: i.name.clone(),
                    category: i.category.clone(),
                    invented: ev.date,
                    adoption: i.adoption_rate.min(1.0),
                });
            }
        }
        _ => {}
    }

    if let Some(summary) = event_summary(ev) {
        if !snap.narrative.is_empty() {
            snap.narrative.push('\n');
        }
        snap.narrative.push_str(&format!("[{}] {}", ev.date.display(), summary));
    }
}

fn event_summary(ev: &HistoryEvent) -> Option<String> {
    match &ev.payload {
        EventPayload::War(w) => {
            if let (Some(end), Some(winner)) = (w.end_date, &w.winner) {
                if end == ev.date {
                    Some(format!("War '{}' ends; {} wins.", w.name, winner))
                } else {
                    None
                }
            } else {
                Some(format!("War '{}' begins: {}", w.name, w.participants.join(", ")))
            }
        }
        EventPayload::Treaty(t) => {
            Some(format!("Treaty '{}': {}", t.name, t.terms))
        }
        EventPayload::Unrest(u) => {
            Some(format!("{} in {} (severity {:.0}/10): {}", u.unrest_kind, u.region, u.severity, u.description))
        }
        EventPayload::NewsSeed(n) => {
            Some(format!("[NEWS] {} ({})", n.headline, n.source))
        }
        EventPayload::Narrative(n) => {
            if n.text.trim().is_empty() {
                None
            } else {
                Some(n.text.clone())
            }
        }
        EventPayload::NationCollapsed(c) => Some(format!("Nation {} collapses.", c.nation)),
        EventPayload::NationFounded(f) => Some(format!("Nation {} founded (capital {}).", f.name, f.capital)),
        EventPayload::BorderChange(b) => Some(format!(
            "{} transfers from {} to {}.",
            b.territory, b.prev_owner, b.new_owner
        )),
        EventPayload::Invention(i) => Some(format!("{} invented in {} ({}).", i.name, i.region, i.category)),
        EventPayload::Census(c) => Some(format!("Census of {}: {}.", c.nation, c.population)),
        EventPayload::Migration(m) => {
            Some(format!("{} migrate from {} to {} ({})", m.amount, m.from_region, m.to_region, m.reason))
        }
        EventPayload::EpochBaseline(_) => None,
    }
}

/// Merge canonical + scenario event lists for a timeline view.
pub fn merge_sorted(canonical: Vec<HistoryEvent>, scenario: Vec<HistoryEvent>) -> Vec<HistoryEvent> {
    let mut all = canonical;
    all.extend(scenario);
    all.sort_by_key(|e| (e.date, e.seq));
    all
}

/// Validate an event payload structurally before storage.
pub fn structural_validate(ev: &HistoryEvent) -> Result<(), EngineError> {
    match &ev.payload {
        EventPayload::Census(c) => {
            if c.population < 0 {
                return Err(EngineError::invalid("census population must be non-negative"));
            }
            for (_k, v) in c.religion_pct.iter().chain(c.ethnicity_pct.iter()) {
                if !v.is_finite() || *v < 0.0 || *v > 100.0 {
                    return Err(EngineError::invalid("percentage must be in 0..=100"));
                }
            }
        }
        EventPayload::Migration(m) => {
            if m.amount < 0 {
                return Err(EngineError::invalid("migration amount must be non-negative"));
            }
        }
        EventPayload::BorderChange(c) => {
            if c.territory.is_empty() || c.new_owner.is_empty() {
                return Err(EngineError::invalid("border change needs territory + owner"));
            }
        }
        EventPayload::Invention(i)
            if (i.year < -9999 || i.year > 9999) => {
                return Err(EngineError::invalid("invention year out of range"));
            }
        _ => {}
    }
    Ok(())
}
