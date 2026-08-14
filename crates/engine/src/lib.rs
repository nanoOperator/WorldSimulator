//! WorldSimulator simulation engine.
//!
//! A local, offline alternate-history and future-prediction engine built on a
//! canonical (immutable) world history that runs from the first controlled
//! use of fire by Homo erectus (~2,000,000 BCE) through to the present day.
//! Scenarios diverge at a user-chosen point; the engine runs an adaptive step
//! loop driven by three local LLMs:
//!   - `mustafakemal` = Qwen3-8B for causal simulation,
//!   - `inalcik` = Qwen2.5-3B for data/statistics,
//!   - `ortayli` = Qwen3-Embedding for semantic retrieval.
//!
//! A deterministic rule-based fallback runs when the GGUF weights are absent.

pub mod apply;
pub mod causal;
pub mod engine;
pub mod events;
pub mod fallback;
pub mod llm;
pub mod models;
pub mod news;
pub mod retrieval;
pub mod scenario;
pub mod state;
pub mod storage;
pub mod validate;

use serde::{Deserialize, Serialize};
use std::fmt;

/// Version of the canonical history dataset shipped with the engine.
pub const CANONICAL_VERSION: &str = "1.0.0";
/// Default day-number zero point used by [`SimDate::days_from_ce`].
pub const DAYS_ZERO_CE: i64 = 719_468;

/// A date in the simulated timeline.
///
/// Uses astronomical year numbering: year 0 == 1 BCE, year -3199 == 3200 BCE.
/// Gregorian proleptic calendar, month 1-12, day 1-31.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SimDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl SimDate {
    /// Construct from astronomical year. `SimDate::from_astro(-3199, 1, 1)` is
    /// 1 Jan 3200 BCE.
    pub fn from_astro(year: i32, month: u8, day: u8) -> Self {
        let month = month.clamp(1, 12);
        let day = day.clamp(1, 28);
        Self { year, month, day }
    }

    /// Construct from a CE year (1 = 1 CE, 0 = 1 BCE).
    pub fn from_ce(year: i32, month: u8, day: u8) -> Self {
        Self::from_astro(year, month, day)
    }

    /// Construct from a BCE year. `from_bce(3200)` == 1 Jan 3200 BCE.
    pub fn from_bce(year: u32, month: u8, day: u8) -> Self {
        Self::from_astro(-(year as i32) + 1, month, day)
    }

    /// Year formatted for display: "3200 BCE" or "1943 CE".
    pub fn display_year(&self) -> String {
        if self.year <= 0 {
            format!("{} BCE", 1 - self.year)
        } else {
            format!("{} CE", self.year)
        }
    }

    /// Full display date.
    pub fn display(&self) -> String {
        format!("{}-{:02}-{:02} ({})", self.year.abs(), self.month, self.day, self.display_year())
    }

    /// Days since 1 Jan 1970 in the proleptic Gregorian calendar (handles BCE).
    pub fn days_from_ce(&self) -> i64 {
        let mut y = self.year as i64;
        let m = self.month as i64;
        let d = self.day as i64;
        y -= if m <= 2 { 1 } else { 0 };
        let era = (if y >= 0 { y } else { y - 399 }) / 400;
        let yoe = y - era * 400;
        let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - DAYS_ZERO_CE
    }

    /// Construct from days since 1 Jan 1970.
    pub fn from_days(days: i64) -> Self {
        let z = days + DAYS_ZERO_CE;
        let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        Self { year: y as i32 + (if m <= 2 { 1 } else { 0 }), month: m as u8, day: d as u8 }
    }

    /// Add `years` (approximated as 365.25 days).
    pub fn add_years(&self, years: f64) -> Self {
        Self::from_days(self.days_from_ce() + (years * 365.25).round() as i64)
    }

    /// Add `months` (approximated as 30.4375 days).
    pub fn add_months(&self, months: i64) -> Self {
        Self::from_days(self.days_from_ce() + months * 30)
    }

    /// Difference to `other` in years (floating, calendar-approximate).
    pub fn years_until(&self, other: &SimDate) -> f64 {
        (other.days_from_ce() - self.days_from_ce()) as f64 / 365.25
    }
}

impl Default for SimDate {
    fn default() -> Self {
        // 2,000,000 BCE: the era of Homo erectus and the first controlled fire.
        SimDate::from_bce(2_000_000, 1, 1)
    }
}

impl fmt::Display for SimDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

/// The fixed start of the canonical timeline: the first controlled use of
/// fire by Homo erectus, roughly two million years ago.
pub const HISTORY_START: SimDate = SimDate { year: -1_999_999, month: 1, day: 1 };

/// The fixed "present" used for live news / future prediction seeds.
pub const PRESENT_YEAR: i32 = 2026;

/// Named historical periods used by the UI scrollers and the seed pipeline.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Era {
    pub name: &'static str,
    pub start: SimDate,
    /// Suggested step size in years when auto-scrolling through this era.
    pub default_step: f64,
}

pub const ERAS: [Era; 14] = [
    Era { name: "Paleolithic / First Fire", start: SimDate { year: -1_999_999, month: 1, day: 1 }, default_step: 10_000.0 },
    Era { name: "Lower Paleolithic", start: SimDate { year: -1_000_000, month: 1, day: 1 }, default_step: 5_000.0 },
    Era { name: "Middle Paleolithic", start: SimDate { year: -300_000, month: 1, day: 1 }, default_step: 1_000.0 },
    Era { name: "Upper Paleolithic", start: SimDate { year: -50_000, month: 1, day: 1 }, default_step: 500.0 },
    Era { name: "Mesolithic / Neolithic", start: SimDate { year: -9_999, month: 1, day: 1 }, default_step: 100.0 },
    Era { name: "Bronze Age / Sumer", start: SimDate { year: -3_199, month: 1, day: 1 }, default_step: 25.0 },
    Era { name: "Classical Antiquity", start: SimDate { year: -779, month: 1, day: 1 }, default_step: 10.0 },
    Era { name: "Middle Ages", start: SimDate { year: 477, month: 1, day: 1 }, default_step: 10.0 },
    Era { name: "Renaissance / Age of Sail", start: SimDate { year: 1401, month: 1, day: 1 }, default_step: 5.0 },
    Era { name: "Industrial Revolution", start: SimDate { year: 1761, month: 1, day: 1 }, default_step: 1.0 },
    Era { name: "Imperial Era", start: SimDate { year: 1871, month: 1, day: 1 }, default_step: 1.0 },
    Era { name: "World Wars", start: SimDate { year: 1914, month: 1, day: 1 }, default_step: 0.5 },
    Era { name: "Cold War", start: SimDate { year: 1947, month: 1, day: 1 }, default_step: 1.0 },
    Era { name: "Contemporary / Future", start: SimDate { year: 1992, month: 1, day: 1 }, default_step: 1.0 },
];

/// The era containing `date`.
pub fn era_of(date: SimDate) -> &'static Era {
    let mut cur = &ERAS[0];
    for e in ERAS.iter() {
        if date >= e.start {
            cur = e;
        } else {
            break;
        }
    }
    cur
}

/// Top-level engine errors.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("storage error: {0}")]
    Storage(String),
    #[error("scenario not found: {0}")]
    ScenarioNotFound(String),
    #[error("edit before divergence point is hard-locked (scenario diverges at {0})")]
    DivergenceLocked(String),
    #[error("invalid world state: {0}")]
    InvalidState(String),
    #[error("LLM error: {0}")]
    Llm(String),
    #[error("model {0} not available (GGUF not found)")]
    ModelUnavailable(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("RSS parse error: {0}")]
    Rss(String),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl EngineError {
    pub fn storage(msg: impl Into<String>) -> Self {
        EngineError::Storage(msg.into())
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        EngineError::InvalidState(msg.into())
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

/// Re-exports for crate consumers (the HTTP server and the desktop shell).
pub use engine::{date_from_iso, Engine, ModelStatus, SimProgress, SimulationOptions};
pub use fallback::detect_divergence;
pub use state::WorldSnapshot;
pub use storage::Scenario;
