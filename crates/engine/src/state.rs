//! World-state model: the materialized view of the world at a given date.
//!
//! The world is event-sourced. A [`WorldSnapshot`] is computed by replaying
//! canonical events plus scenario events up to a date. Snapshots are the
//! immutable "world" handed to the LLM and rendered on the map.

use crate::SimDate;
use serde::{Deserialize, Serialize};

/// A nation in the world snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nation {
    pub id: String,
    pub name: String,
    pub color: String,
    pub population: i64,
    /// Religion -> percent (sums to ~100).
    pub religion_pct: Vec<(String, f64)>,
    /// Ethnicity -> percent (sums to ~100).
    pub ethnicity_pct: Vec<(String, f64)>,
    pub economy_index: f64,
    pub military_index: f64,
    /// Territory ids currently owned.
    pub territories: Vec<String>,
}

impl Nation {
    pub fn religion(&self) -> &[(String, f64)] {
        &self.religion_pct
    }
}

/// A territorial polygon in the snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Territory {
    /// Stable identifier (ISO-3 or historical region code).
    pub id: String,
    pub name: String,
    /// Current owner nation id.
    pub owner: String,
    /// GeoJSON geometry (Polygon/MultiPolygon) for map rendering.
    pub geometry_geojson: String,
}

/// A technology present in the snapshot, with adoption level 0.0-1.0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TechState {
    pub tech_id: String,
    pub name: String,
    pub category: String,
    pub invented: SimDate,
    pub adoption: f64,
}

/// Immutable world state at a date.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldSnapshot {
    pub date: SimDate,
    pub nations: Vec<Nation>,
    pub territories: Vec<Territory>,
    pub techs: Vec<TechState>,
    /// Free-text historical narrative accumulated to this date.
    pub narrative: String,
}

impl WorldSnapshot {
    pub fn nation(&self, id: &str) -> Option<&Nation> {
        self.nations.iter().find(|n| n.id == id)
    }

    pub fn total_population(&self) -> i64 {
        self.nations.iter().map(|n| n.population).sum()
    }

    /// GeoJSON FeatureCollection of all territories, colored by owner.
    pub fn to_geojson(&self) -> serde_json::Value {
        let features: Vec<serde_json::Value> = self
            .territories
            .iter()
            .map(|t| {
                let owner = self.nation(&t.owner);
                let mut props = serde_json::Map::new();
                props.insert("id".into(), serde_json::json!(t.id));
                props.insert("name".into(), serde_json::json!(t.name));
                props.insert("owner".into(), serde_json::json!(t.owner));
                props.insert(
                    "ownerName".into(),
                    serde_json::json!(owner.map(|o| o.name.as_str()).unwrap_or("unclaimed")),
                );
                props.insert(
                    "color".into(),
                    serde_json::json!(owner.map(|o| o.color.as_str()).unwrap_or("#888888")),
                );
                serde_json::json!({
                    "type": "Feature",
                    "properties": props,
                    "geometry": serde_json::from_str::<serde_json::Value>(&t.geometry_geojson)
                        .unwrap_or(serde_json::json!(null)),
                })
            })
            .collect();
        serde_json::json!({ "type": "FeatureCollection", "features": features })
    }
}

/// A named color palette used to assign stable colors to nations.
/// 48 visually-distinct hues with enough separation that adjacent countries
/// almost never share the same color (the hash spreads them evenly).
pub fn color_for_nation(id: &str) -> String {
    const PALETTE: [&str; 48] = [
        "#c0392b", "#e74c3c", "#e67e22", "#f39c12", "#f1c40f", "#d4ac0d",
        "#27ae60", "#2ecc71", "#1abc9c", "#16a085", "#2980b9", "#3498db",
        "#8e44ad", "#9b59b6", "#2c3e50", "#34495e", "#c0392b", "#e91e63",
        "#ff5722", "#ff9800", "#ffc107", "#8bc34a", "#4caf50", "#009688",
        "#00bcd4", "#03a9f4", "#673ab7", "#9c27b0", "#607d8b", "#795548",
        "#d32f2f", "#1976d2", "#388e3c", "#f57c00", "#7b1fa2", "#0097a7",
        "#c62828", "#ad1457", "#6a1b9a", "#283593", "#00695c", "#2e7d32",
        "#e65100", "#bf360c", "#4e342e", "#37474f", "#558b2f", "#0277bd",
    ];
    let mut h = 5381u64;
    for b in id.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    // Mix high and low bits to spread similar IDs further apart.
    h ^= h >> 17;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 31;
    PALETTE[(h % PALETTE.len() as u64) as usize].to_string()
}
