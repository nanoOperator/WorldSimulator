//! Causal log: every applied scenario event records its parent events so a
//! divergence can be traced back step by step.

use crate::events::HistoryEvent;
use crate::storage::Storage;
use crate::Result;

/// Resolve the causal chain for an event (oldest -> newest) within a branch.
pub fn causal_chain(
    storage: &Storage,
    scenario_id: &str,
    branch_id: &str,
    event_id: i64,
) -> Result<Vec<HistoryEvent>> {
    let mut chain = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut current = Some(event_id);
    while let Some(id) = current {
        if !visited.insert(id) {
            break;
        }
        let ev = storage
            .scenario_events_up_to(scenario_id, branch_id, crate::SimDate { year: 9999, month: 12, day: 28 })
            .unwrap_or_default()
            .into_iter()
            .find(|e| e.id == id);
        match ev {
            Some(e) => {
                let parents = e.causal_parents.clone();
                chain.push(e);
                current = parents.first().copied();
            }
            None => break,
        }
    }
    chain.reverse();
    Ok(chain)
}

/// Render a causal chain as readable text.
pub fn render_chain(chain: &[HistoryEvent]) -> String {
    let mut out = String::new();
    for (i, ev) in chain.iter().enumerate() {
        out.push_str(&format!(
            "{} {} [{}]\n    {}\n",
            i + 1,
            ev.date.display(),
            ev.title,
            ev.body
        ));
    }
    out
}
