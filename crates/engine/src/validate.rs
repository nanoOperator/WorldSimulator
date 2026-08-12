//! Constraint validation, auto-fix, and LLM self-check.
//!
//! The engine runs every proposed event through [`validate`]; simple
//! violations are auto-fixed by [`auto_fix`], and the remaining ones are
//! reported back to the model for a retry. When two model passes disagree,
//! [`self_check`] resolves the numbers.

use crate::events::{EventPayload, HistoryEvent, HistoryEvent as HE};
use crate::state::WorldSnapshot;
use crate::SimDate;
use std::collections::HashMap;

/// A constraint violation found in a proposed event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Violation {
    pub event_title: String,
    pub message: String,
}

/// Run all structural + world-context validations.
pub fn validate(
    ev: &HistoryEvent,
    divergence: SimDate,
    world_before: &WorldSnapshot,
) -> Vec<Violation> {
    let mut v = Vec::new();
    if ev.date < divergence {
        v.push(Violation {
            event_title: ev.title.clone(),
            message: format!("date {} is before the divergence point {}", ev.date, divergence),
        });
    }
    if let Err(e) = crate::apply::structural_validate(ev) {
        v.push(Violation {
            event_title: ev.title.clone(),
            message: e.to_string(),
        });
    }
    match &ev.payload {
        EventPayload::Census(c) => {
            let sum: f64 = c.religion_pct.values().sum();
            if (sum - 100.0).abs() > 1.0 {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: format!("religion percentages sum to {sum:.1}, expected ~100"),
                });
            }
            let esum: f64 = c.ethnicity_pct.values().sum();
            if (esum - 100.0).abs() > 1.0 {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: format!("ethnicity percentages sum to {esum:.1}, expected ~100"),
                });
            }
            if world_before.nation(&c.nation).is_none() && !caused_nation_exists(ev) {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: format!("census references unknown nation '{}'", c.nation),
                });
            }
        }
        EventPayload::BorderChange(c) => {
            if world_before.nation(&c.new_owner).is_none() && c.new_owner != "_unclaimed_" {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: format!("border change targets unknown owner '{}'", c.new_owner),
                });
            }
            if !world_before.territories.iter().any(|t| t.id == c.territory)
                && c.geometry_geojson.as_ref().map_or(true, |g| g.is_empty())
            {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: format!(
                        "territory '{}' does not exist and carries no new geometry",
                        c.territory
                    ),
                });
            }
        }
        EventPayload::Migration(m) => {
            if let Some(n) = world_before.nation(&m.from_region) {
                if n.population < m.amount {
                    v.push(Violation {
                        event_title: ev.title.clone(),
                        message: format!(
                            "migration of {} exceeds source population {}",
                            m.amount, n.population
                        ),
                    });
                }
            }
        }
        EventPayload::War(w) => {
            if w.start_date > ev.date && ev.date != w.start_date {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: "war start date is in the future of the event date".into(),
                });
            }
            if let Some(end) = w.end_date {
                if end < w.start_date {
                    v.push(Violation {
                        event_title: ev.title.clone(),
                        message: "war end date precedes start date".into(),
                    });
                }
            }
        }
        EventPayload::Invention(i) => {
            if i.adoption_rate < 0.0 || i.adoption_rate > 1.0 {
                v.push(Violation {
                    event_title: ev.title.clone(),
                    message: "invention adoption_rate must be in 0.0..=1.0".into(),
                });
            }
        }
        _ => {}
    }
    v
}

fn caused_nation_exists(ev: &HistoryEvent) -> bool {
    // Census events may legitimately follow a NationFounded in the same step;
    // treat as valid if the nation is founded by the same seq.
    ev.title.contains("founded") || ev.title.contains("Founded")
}

/// Auto-fix simple violations in place. Returns true if anything changed.
pub fn auto_fix(ev: &mut HistoryEvent) -> bool {
    let mut changed = false;
    match &mut ev.payload {
        EventPayload::Census(c) => {
            if c.population < 0 {
                c.population = 0;
                changed = true;
            }
            normalize_pct(&mut c.religion_pct, &mut changed);
            normalize_pct(&mut c.ethnicity_pct, &mut changed);
            if c.economy_index < 0.0 {
                c.economy_index = 0.0;
                changed = true;
            }
            if c.military_index < 0.0 {
                c.military_index = 0.0;
                changed = true;
            }
        }
        EventPayload::Migration(m) => {
            if m.amount < 0 {
                m.amount = 0;
                changed = true;
            }
        }
        EventPayload::Invention(i) => {
            if i.adoption_rate < 0.0 {
                i.adoption_rate = 0.0;
                changed = true;
            }
            if i.adoption_rate > 1.0 {
                i.adoption_rate = 1.0;
                changed = true;
            }
        }
        EventPayload::Unrest(u) => {
            if u.severity < 0.0 {
                u.severity = 0.0;
                changed = true;
            }
            if u.severity > 10.0 {
                u.severity = 10.0;
                changed = true;
            }
        }
        _ => {}
    }
    changed
}

fn normalize_pct(pct: &mut HashMap<String, f64>, changed: &mut bool) {
    let sum: f64 = pct.values().sum();
    if sum > 0.0 && (sum - 100.0).abs() > 1.0 {
        let factor = 100.0 / sum;
        for v in pct.values_mut() {
            *v *= factor;
        }
        *changed = true;
    }
}

/// Compare the numbers produced by two model passes (self-check). Returns a
/// list of resolved/flagged discrepancies.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelfCheckDiff {
    pub event_title: String,
    pub field: String,
    pub first: f64,
    pub second: f64,
    /// true when the second pass (authoritative) won.
    pub resolved_to_second: bool,
}

pub fn self_check(first: &[HistoryEvent], second: &[HistoryEvent]) -> Vec<SelfCheckDiff> {
    let mut diffs = Vec::new();
    for (a, b) in first.iter().zip(second.iter()) {
        let pa = numbers(a);
        let pb = numbers(b);
        for (k, va) in pa {
            if let Some(vb) = pb.get(&k) {
                let rel = (va - vb).abs() / va.max(1.0);
                if rel > 0.10 {
                    diffs.push(SelfCheckDiff {
                        event_title: a.title.clone(),
                        field: k,
                        first: va,
                        second: *vb,
                        resolved_to_second: true,
                    });
                }
            }
        }
    }
    diffs
}

fn numbers(ev: &HistoryEvent) -> HashMap<String, f64> {
    match &ev.payload {
        EventPayload::Census(c) => {
            let mut m = HashMap::new();
            m.insert("population".into(), c.population as f64);
            m.insert("economy".into(), c.economy_index);
            m.insert("military".into(), c.military_index);
            m
        }
        EventPayload::Migration(m) => {
            let mut x = HashMap::new();
            x.insert("amount".into(), m.amount as f64);
            x
        }
        _ => HashMap::new(),
    }
}

/// Type alias kept for clarity.
#[allow(dead_code)]
type Event = HE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Census;
    use std::collections::HashMap;

    fn census(pop: i64, religions: Vec<(&str, f64)>) -> HistoryEvent {
        HistoryEvent {
            id: 0,
            date: SimDate::from_ce(1950, 1, 1),
            scenario_id: None,
            title: "census".into(),
            body: String::new(),
            payload: EventPayload::Census(Census {
                nation: "USA".into(),
                population: pop,
                religion_pct: religions
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect::<HashMap<_, _>>(),
                ethnicity_pct: HashMap::new(),
                economy_index: 1.0,
                military_index: 1.0,
                caused_by: vec![],
            }),
            source_model: "test".into(),
            causal_parents: vec![],
            seq: 1,
        }
    }

    #[test]
    fn auto_fix_normalizes_percentages_and_clamps_negative() {
        let mut ev = census(
            -5,
            vec![("Christianity", 60.0), ("Islam", 40.0), ("Other", 30.0)],
        );
        auto_fix(&mut ev);
        if let EventPayload::Census(c) = &ev.payload {
            assert_eq!(c.population, 0);
            let sum: f64 = c.religion_pct.values().sum();
            assert!((sum - 100.0).abs() < 1.0);
        } else {
            panic!("expected census");
        }
    }
}
