//! SQLite persistence layer.
//!
//! The canonical timeline is immutable; scenario events are stored per
//! scenario/branch and applied on top. World snapshots are computed by
//! replaying events through [`crate::apply::apply`].

use crate::apply;
use crate::events::{EventPayload, HistoryEvent};
use crate::state::WorldSnapshot;
use crate::{Result, SimDate};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// A scenario: a named divergence from the canonical timeline.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub divergence: SimDate,
    pub created_at: String,
}

/// A simulation branch within a scenario.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Branch {
    pub id: String,
    pub scenario_id: String,
    pub parent_id: Option<String>,
    pub seed: i64,
    /// pending | running | done | failed
    pub status: String,
    pub created_at: String,
}

/// A row from the news_items table.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NewsItemRow {
    pub id: String,
    pub source_id: String,
    pub title: String,
    pub link: String,
    pub published_day: i64,
    pub summary: String,
    pub confidence: f64,
    pub processed: bool,
}

pub struct Storage {
    conn: Connection,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let s = Storage { conn };
        s.init_schema()?;
        Ok(s)
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let s = Storage { conn };
        s.init_schema()?;
        Ok(s)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS canonical_events (
                id INTEGER PRIMARY KEY,
                date_day INTEGER NOT NULL,
                date_year INTEGER NOT NULL,
                date_month INTEGER NOT NULL,
                date_dayofmonth INTEGER NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                payload TEXT NOT NULL,
                seq INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_canon_date ON canonical_events(date_day);

            CREATE TABLE IF NOT EXISTS scenarios (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                prompt TEXT NOT NULL,
                divergence_day INTEGER NOT NULL,
                divergence_year INTEGER NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS branches (
                id TEXT PRIMARY KEY,
                scenario_id TEXT NOT NULL,
                parent_id TEXT,
                seed INTEGER NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_branch_scenario ON branches(scenario_id);

            CREATE TABLE IF NOT EXISTS scenario_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scenario_id TEXT NOT NULL,
                branch_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                date_day INTEGER NOT NULL,
                date_year INTEGER NOT NULL,
                date_month INTEGER NOT NULL,
                date_dayofmonth INTEGER NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL DEFAULT '',
                payload TEXT NOT NULL,
                source_model TEXT NOT NULL,
                causal_parents TEXT NOT NULL DEFAULT '[]',
                meta TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_scenev_date
                ON scenario_events(scenario_id, branch_id, date_day);
            CREATE INDEX IF NOT EXISTS idx_scenev_seq
                ON scenario_events(scenario_id, branch_id, seq);

            CREATE TABLE IF NOT EXISTS news_sources (
                id TEXT PRIMARY KEY,
                url TEXT NOT NULL,
                base_trust REAL NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS news_items (
                id TEXT PRIMARY KEY,
                source_id TEXT NOT NULL,
                title TEXT NOT NULL,
                link TEXT NOT NULL,
                published_day INTEGER NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                confidence REAL NOT NULL,
                processed INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_news_conf ON news_items(confidence);
            "#,
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- meta

    pub fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO meta(key,value) VALUES(?1,?2)
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![key, value],
            )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let v = self
            .conn
            .query_row("SELECT value FROM meta WHERE key=?1", params![key], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(v)
    }

    // -------------------------------------------------------- canonical log

    pub fn add_canonical_event(&self, ev: &HistoryEvent) -> Result<i64> {
        let payload = serde_json::to_string(&ev.payload)?;
        self.conn.execute(
            "INSERT INTO canonical_events
               (id, date_day, date_year, date_month, date_dayofmonth,
                title, body, payload, seq)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                ev.id,
                ev.date.days_from_ce(),
                ev.date.year,
                ev.date.month,
                ev.date.day,
                ev.title,
                ev.body,
                payload,
                ev.seq,
            ],
        )?;
        Ok(ev.id)
    }

    pub fn canonical_event_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM canonical_events", [], |r| r.get(0))?)
    }

    fn row_to_event(
        &self,
        id: i64,
        day: i64,
        title: String,
        body: String,
        payload_json: String,
        seq: i64,
    ) -> Result<HistoryEvent> {
        let date = SimDate::from_days(day);
        let payload: EventPayload = serde_json::from_str(&payload_json)
            .map_err(|e| crate::EngineError::Storage(format!("bad payload row {id}: {e}")))?;
        Ok(HistoryEvent {
            id,
            date,
            scenario_id: None,
            title,
            body,
            payload,
            source_model: "canonical".into(),
            causal_parents: vec![],
            seq,
        })
    }

    pub fn canonical_events_up_to(&self, date: SimDate) -> Result<Vec<HistoryEvent>> {
        let day = date.days_from_ce();
        let mut stmt = self.conn.prepare(
            "SELECT id, date_day, title, body, payload, seq
             FROM canonical_events WHERE date_day <= ?1 ORDER BY date_day, seq",
        )?;
        let rows = stmt.query_map(params![day], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, day, title, body, payload, seq) = row?;
            out.push(self.row_to_event(id, day, title, body, payload, seq)?);
        }
        Ok(out)
    }

    // ------------------------------------------------------------ scenarios

    pub fn create_scenario(
        &self,
        id: &str,
        name: &str,
        prompt: &str,
        divergence: SimDate,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO scenarios
               (id, name, prompt, divergence_day, divergence_year, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                id,
                name,
                prompt,
                divergence.days_from_ce(),
                divergence.year,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_scenarios(&self) -> Result<Vec<Scenario>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, prompt, divergence_day, created_at FROM scenarios ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            let (id, name, prompt, day, created) = r?;
            out.push(Scenario {
                id,
                name,
                prompt,
                divergence: SimDate::from_days(day),
                created_at: created,
            });
        }
        Ok(out)
    }

    pub fn get_scenario(&self, id: &str) -> Result<Option<Scenario>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, name, prompt, divergence_day, created_at FROM scenarios WHERE id=?1",
                params![id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.map(|(id, name, prompt, day, created)| Scenario {
            id,
            name,
            prompt,
            divergence: SimDate::from_days(day),
            created_at: created,
        }))
    }

    pub fn delete_scenario(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM scenario_events WHERE scenario_id=?1", params![id])?;
        self.conn
            .execute("DELETE FROM branches WHERE scenario_id=?1", params![id])?;
        self.conn
            .execute("DELETE FROM scenarios WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn update_scenario(&self, id: &str, name: &str, prompt: &str) -> Result<()> {
        self.conn
            .execute("UPDATE scenarios SET name=?2, prompt=?3 WHERE id=?1", params![id, name, prompt])?;
        Ok(())
    }

    // -------------------------------------------------------------- branches

    pub fn create_branch(&self, branch: &Branch) -> Result<()> {
        self.conn.execute(
            "INSERT INTO branches (id, scenario_id, parent_id, seed, status, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![
                branch.id,
                branch.scenario_id,
                branch.parent_id,
                branch.seed,
                branch.status,
                branch.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_branches(&self, scenario_id: &str) -> Result<Vec<Branch>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, scenario_id, parent_id, seed, status, created_at FROM branches WHERE scenario_id=?1 ORDER BY created_at")?;
        let rows = stmt.query_map(params![scenario_id], |r| {
            Ok(Branch {
                id: r.get(0)?,
                scenario_id: r.get(1)?,
                parent_id: r.get(2)?,
                seed: r.get(3)?,
                status: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn set_branch_status(&self, id: &str, status: &str) -> Result<()> {
        self.conn
            .execute("UPDATE branches SET status=?2 WHERE id=?1", params![id, status])?;
        Ok(())
    }

    // -------------------------------------------------------- scenario events

    /// Insert a scenario event, enforcing the hard divergence lock: the event
    /// date must be >= the scenario divergence point.
    pub fn add_scenario_event(&self, ev: &HistoryEvent, branch_id: &str) -> Result<i64> {
        let scenario_id = ev
            .scenario_id
            .as_deref()
            .ok_or_else(|| crate::EngineError::invalid("scenario event missing scenario_id"))?;
        let scenario = self
            .get_scenario(scenario_id)?
            .ok_or_else(|| crate::EngineError::ScenarioNotFound(scenario_id.into()))?;
        if ev.date < scenario.divergence {
            return Err(crate::EngineError::DivergenceLocked(
                scenario.divergence.display(),
            ));
        }
        self.add_scenario_event_inner(ev, branch_id)
    }

    pub fn add_scenario_event_for_branch(
        &self,
        ev: &HistoryEvent,
        scenario_id: &str,
        branch_id: &str,
    ) -> Result<i64> {
        let mut ev = ev.clone();
        ev.scenario_id = Some(scenario_id.into());
        self.add_scenario_event(&ev, branch_id)
    }

    fn add_scenario_event_inner(&self, ev: &HistoryEvent, branch_id: &str) -> Result<i64> {
        let payload = serde_json::to_string(&ev.payload)?;
        let parents = serde_json::to_string(&ev.causal_parents)?;
        let meta = serde_json::json!({
            "model": ev.source_model,
        })
        .to_string();
        self.conn.execute(
            "INSERT INTO scenario_events
               (scenario_id, branch_id, seq, date_day, date_year, date_month,
                date_dayofmonth, title, body, payload, source_model,
                causal_parents, meta)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                ev.scenario_id,
                branch_id,
                ev.seq,
                ev.date.days_from_ce(),
                ev.date.year,
                ev.date.month,
                ev.date.day,
                ev.title,
                ev.body,
                payload,
                ev.source_model,
                parents,
                meta,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn scenario_events_up_to(
        &self,
        scenario_id: &str,
        branch_id: &str,
        date: SimDate,
    ) -> Result<Vec<HistoryEvent>> {
        let day = date.days_from_ce();
        let mut stmt = self.conn.prepare(
            "SELECT id, scenario_id, date_day, title, body, payload, source_model,
                    causal_parents, seq
             FROM scenario_events
             WHERE scenario_id=?1 AND branch_id=?2 AND date_day<=?3
             ORDER BY date_day, seq",
        )?;
        let rows = stmt.query_map(params![scenario_id, branch_id, day], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, scid, day, title, body, payload, model, parents, seq) = row?;
            let date = SimDate::from_days(day);
            let payload: EventPayload = serde_json::from_str(&payload)
                .map_err(|e| crate::EngineError::Storage(format!("bad payload {id}: {e}")))?;
            let parents: Vec<i64> = serde_json::from_str(&parents).unwrap_or_default();
            out.push(HistoryEvent {
                id,
                date,
                scenario_id: Some(scid),
                title,
                body,
                payload,
                source_model: model,
                causal_parents: parents,
                seq,
            });
        }
        Ok(out)
    }

    pub fn last_scenario_seq(&self, scenario_id: &str, branch_id: &str) -> Result<i64> {
        let v: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(seq) FROM scenario_events WHERE scenario_id=?1 AND branch_id=?2",
                params![scenario_id, branch_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.unwrap_or(0))
    }

    // ------------------------------------------------------------------ news

    pub fn add_news_source(&self, id: &str, url: &str, base_trust: f64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO news_sources(id, url, base_trust, enabled)
             VALUES (?1,?2,?3,1)",
            params![id, url, base_trust],
        )?;
        Ok(())
    }

    pub fn list_news_sources(&self) -> Result<Vec<(String, String, f64, bool)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, url, base_trust, enabled FROM news_sources ORDER BY base_trust DESC")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn add_news_item(
        &self,
        id: &str,
        source_id: &str,
        title: &str,
        link: &str,
        published_day: i64,
        summary: &str,
        confidence: f64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO news_items
               (id, source_id, title, link, published_day, summary, confidence, processed)
             VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
            params![id, source_id, title, link, published_day, summary, confidence],
        )?;
        Ok(())
    }

    /// Top news items by confidence, newest first.
    pub fn top_news_items(&self, limit: usize) -> Result<Vec<NewsItemRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_id, title, link, published_day, summary, confidence, processed
             FROM news_items ORDER BY confidence DESC, published_day DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(NewsItemRow {
                id: r.get(0)?,
                source_id: r.get(1)?,
                title: r.get(2)?,
                link: r.get(3)?,
                published_day: r.get(4)?,
                summary: r.get(5)?,
                confidence: r.get(6)?,
                processed: r.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn unprocessed_news_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM news_items WHERE processed=0", [], |r| r.get(0))?)
    }

    pub fn mark_news_processed(&self, id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE news_items SET processed=1 WHERE id=?1", params![id])?;
        Ok(())
    }

    // ------------------------------------------------------------- snapshots

    /// Build the world snapshot at `date` for a scenario branch (or canonical
    /// only when scenario_id/branch_id are None).
    ///
    /// Territory geometry is carried by `EpochBaseline` events (via
    /// `apply::apply`); here we re-resolve ownership from the nation list so
    /// divergence border changes are reflected on the map.
    pub fn build_snapshot(
        &self,
        date: SimDate,
        scenario_id: Option<&str>,
        branch_id: Option<&str>,
    ) -> Result<WorldSnapshot> {
        let mut snap = WorldSnapshot { date, ..Default::default() };
        for ev in self.canonical_events_up_to(date)? {
            apply::apply(&mut snap, &ev);
        }
        if let (Some(sc), Some(br)) = (scenario_id, branch_id) {
            for ev in self.scenario_events_up_to(sc, br, date)? {
                apply::apply(&mut snap, &ev);
            }
        }
        for t in snap.territories.iter_mut() {
            let owner = snap
                .nations
                .iter()
                .find(|n| n.territories.iter().any(|id| id == &t.id))
                .map(|n| n.id.clone());
            if let Some(o) = owner {
                t.owner = o;
            }
        }
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{Census, EventPayload};
    use std::collections::HashMap;

    fn census(nation: &str, pop: i64) -> HistoryEvent {
        HistoryEvent {
            id: 0,
            date: SimDate::from_ce(1900, 1, 1),
            scenario_id: None,
            title: format!("census {nation}"),
            body: String::new(),
            payload: EventPayload::Census(Census {
                nation: nation.into(),
                population: pop,
                religion_pct: HashMap::new(),
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
    fn scenario_hard_lock_prevents_predivergence_writes() {
        let s = Storage::open_in_memory().unwrap();
        s.create_scenario("s1", "test", "what if", SimDate::from_ce(1943, 6, 1))
            .unwrap();
        let mut ev = census("USA", 130_000_000);
        ev.date = SimDate::from_ce(1930, 1, 1);
        ev.scenario_id = Some("s1".into());
        let err = s.add_scenario_event(&ev, "b1").unwrap_err();
        assert!(matches!(err, crate::EngineError::DivergenceLocked(_)));
    }

    #[test]
    fn snapshot_replays_events() {
        let s = Storage::open_in_memory().unwrap();
        let mut ev = census("FR", 40_000_000);
        ev.id = 1;
        ev.title = "census fr".into();
        ev.seq = 1;
        s.add_canonical_event(&ev).unwrap();
        let snap = s
            .build_snapshot(SimDate::from_ce(1950, 1, 1), None, None)
            .unwrap();
        assert_eq!(snap.total_population(), 40_000_000);
    }
}
