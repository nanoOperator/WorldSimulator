//! Structured history events. Every event carries a date, a kind, and a typed
//! payload serialized as JSON. Canonical events come from the real timeline;
//! scenario events are produced by the simulation and recorded with full
//! causal provenance.

use crate::state::{Nation, TechState, Territory};
use crate::SimDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Discriminated event payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventPayload {
    /// A full replacement of the world at a date (canonical epoch baseline).
    EpochBaseline(EpochBaseline),
    /// A territorial handover or border adjustment.
    BorderChange(BorderChange),
    /// A new nation is founded.
    NationFounded(NationFounded),
    /// A nation collapses / ceases to exist.
    NationCollapsed(NationCollapsed),
    /// A war breaks out or ends; outcome recorded on termination.
    War(War),
    /// A treaty or peace agreement.
    Treaty(Treaty),
    /// An invention / technological milestone.
    Invention(Invention),
    /// A census / demographic snapshot for a nation.
    Census(Census),
    /// A quantified migration flow between regions.
    Migration(Migration),
    /// Civil unrest: riot, guerrilla campaign, rebellion, civil war.
    Unrest(Unrest),
    /// A news item converted into a state seed (future prediction).
    NewsSeed(NewsSeed),
    /// Free-text narrative with structured statistical annotations.
    Narrative(Narrative),
}

/// A full world snapshot used to seed canonical epochs and future seeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochBaseline {
    pub nations: Vec<Nation>,
    pub territories: Vec<Territory>,
    pub techs: Vec<TechState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BorderChange {
    /// Territory identifier (ISO-3 or historical region code).
    pub territory: String,
    /// New owner nation identifier.
    pub new_owner: String,
    /// Previous owner nation identifier.
    pub prev_owner: String,
    /// Optional new geometry as GeoJSON string (set on large redraws).
    #[serde(default)]
    pub geometry_geojson: Option<String>,
    /// Event/step that caused this change (for the causal log).
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NationFounded {
    pub id: String,
    pub name: String,
    /// Initial capital / seat.
    pub capital: String,
    /// Optional color hex used on the map.
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NationCollapsed {
    pub nation: String,
    #[serde(default)]
    pub successor: Option<String>,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct War {
    pub name: String,
    pub participants: Vec<String>,
    pub start_date: SimDate,
    /// Set when the war ends.
    #[serde(default)]
    pub end_date: Option<SimDate>,
    /// Winner nation id, set on end.
    #[serde(default)]
    pub winner: Option<String>,
    /// Qualitative intensity.
    #[serde(default)]
    pub intensity: u8,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Treaty {
    pub name: String,
    pub parties: Vec<String>,
    /// Territory transfers performed under the treaty.
    #[serde(default)]
    pub transfers: Vec<BorderChange>,
    #[serde(default)]
    pub terms: String,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invention {
    pub name: String,
    pub tech_id: String,
    /// Region/nation where invented.
    pub region: String,
    /// Year (astronomical) of invention.
    pub year: i32,
    /// Adoption speed 0.0-1.0 (fraction of region adopting per year at peak).
    #[serde(default = "default_adoption")]
    pub adoption_rate: f64,
    /// Category: military, transport, energy, computing, medical, ...
    pub category: String,
    /// Impact factor used to shift other real inventions.
    #[serde(default = "default_impact")]
    pub impact: f64,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

fn default_adoption() -> f64 {
    0.5
}
fn default_impact() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Census {
    pub nation: String,
    pub population: i64,
    /// Religion -> percent (0-100).
    #[serde(default)]
    pub religion_pct: HashMap<String, f64>,
    /// Ethnicity -> percent (0-100).
    #[serde(default)]
    pub ethnicity_pct: HashMap<String, f64>,
    /// Economic output index (relative scale).
    #[serde(default)]
    pub economy_index: f64,
    /// Military strength index.
    #[serde(default)]
    pub military_index: f64,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Migration {
    pub from_region: String,
    pub to_region: String,
    pub amount: i64,
    pub reason: String,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unrest {
    pub region: String,
    /// riot, guerrilla, rebellion, civil_war, coup
    #[serde(rename = "unrest_kind")]
    pub unrest_kind: String,
    pub severity: f64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsSeed {
    pub headline: String,
    pub source: String,
    pub url: String,
    pub published: SimDate,
    pub confidence: f64,
    /// Nation affected (if any).
    #[serde(default)]
    pub nation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Narrative {
    pub text: String,
    /// Statistical annotations keyed by nation id.
    #[serde(default)]
    pub stats: HashMap<String, HashMap<String, f64>>,
    #[serde(default)]
    pub caused_by: Vec<String>,
}

/// A fully materialized history event as stored/returned to callers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    /// Event row id (canonical events and scenario events share id space
    /// within a scenario query; `scenario_id` distinguishes provenance).
    pub id: i64,
    pub date: SimDate,
    /// Canonical events have scenario_id == None; scenario events Some(id).
    #[serde(default)]
    pub scenario_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub payload: EventPayload,
    /// Which model produced this event.
    #[serde(default)]
    pub source_model: String,
    /// Parent event ids (causal chain).
    #[serde(default)]
    pub causal_parents: Vec<i64>,
    /// Seq number within its timeline (ordering).
    pub seq: i64,
}

/// Kinds of civil unrest, used by validation and the fallback sim.
pub const UNREST_KINDS: [&str; 5] = ["riot", "guerrilla", "rebellion", "civil_war", "coup"];

