//! Live news ingestion: RSS aggregation with trust scoring, and conversion
//! of news items into world-state seeds for future prediction.

use crate::storage::Storage;
use crate::{EngineError, Result, SimDate};
use quick_xml::de::from_str;
use serde::Deserialize;
use std::time::Duration;

/// RSS 2.0 channel (also parses Atom feeds loosely via channel fallbacks).
#[derive(Debug, Deserialize, Default)]
struct Rss {
    #[serde(default)]
    channel: Option<Channel>,
    #[serde(default)]
    entries: Option<Vec<Entry>>,
}

#[derive(Debug, Deserialize, Default)]
struct Channel {
    #[serde(default)]
    item: Vec<Item>,
}

#[derive(Debug, Deserialize, Default)]
struct Item {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    published: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Entry {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    published: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// A default set of RSS sources with base trust scores (0.0-1.0).
pub fn default_sources() -> Vec<(String, String, f64)> {
    vec![
        ("reuters".into(), "https://www.reutersagency.com/feed/?best-topics=business-finance".into(), 0.95),
        ("ap".into(), "http://hosted2.ap.org/atom/APDefault/".into(), 0.93),
        ("bbc-world".into(), "https://feeds.bbci.co.uk/news/world/rss.xml".into(), 0.90),
        ("aljazeera".into(), "https://www.aljazeera.com/xml/rss/all.xml".into(), 0.80),
        ("guardian".into(), "https://www.theguardian.com/world/rss".into(), 0.85),
    ]
}

/// Fetch one RSS/Atom feed and store its items with trust-adjusted scores.
pub fn fetch_source(storage: &Storage, source_id: &str, url: &str, base_trust: f64) -> Result<usize> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("WorldSimulator/1.0 (+https://github.com/nanoOperator/WorldSimulator)")
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| EngineError::Http(e.to_string()))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| EngineError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(EngineError::Http(format!("{} -> {}", url, resp.status())));
    }
    let xml = resp
        .text()
        .map_err(|e| EngineError::Http(e.to_string()))?;
    let parsed: Rss = from_str(&xml).map_err(|e| EngineError::Rss(e.to_string()))?;

    let today = SimDate::from_ce(crate::PRESENT_YEAR, 1, 1).days_from_ce();
    let mut stored = 0usize;

    let mut items: Vec<(String, Option<String>, Option<String>, Option<String>)> = Vec::new();
    if let Some(ch) = &parsed.channel {
        for it in &ch.item {
            items.push((
                it.title.clone().unwrap_or_default(),
                it.link.clone(),
                it.pub_date.clone().or_else(|| it.published.clone()),
                it.description.clone().or_else(|| it.summary.clone()),
            ));
        }
    }
    for e in parsed.entries.unwrap_or_default() {
        items.push((
            e.title.clone().unwrap_or_default(),
            e.link.clone(),
            e.published.clone(),
            e.summary.clone(),
        ));
    }

    for (title, link, date, summary) in items {
        if title.trim().is_empty() {
            continue;
        }
        let link = link.unwrap_or_else(|| format!("{url}#{title}"));
        let published = parse_rss_date(&date).unwrap_or(today);
        let recency = recency_factor(published, today);
        let confidence = (base_trust * 0.7 + recency * 0.3).clamp(0.0, 1.0);
        let id = format!("{}-{:x}", source_id, stable_hash(&(title.clone() + &link)));
        storage.add_news_item(&id, source_id, &title, &link, published, summary.as_deref().unwrap_or(""), confidence)?;
        stored += 1;
    }
    Ok(stored)
}

/// Fetch and store all enabled sources.
pub fn fetch_all(storage: &Storage) -> Result<usize> {
    let sources = storage.list_news_sources()?;
    let mut total = 0usize;
    for (id, url, trust, enabled) in sources {
        if !enabled {
            continue;
        }
        if let Ok(n) = fetch_source(storage, &id, &url, trust) {
            total += n;
        }
    }
    Ok(total)
}

/// Trust-adjusted confidence for a stored item.
pub fn trust_score(base_trust: f64, published_day: i64, today: i64) -> f64 {
    let recency = recency_factor(published_day, today);
    (base_trust * 0.7 + recency * 0.3).clamp(0.0, 1.0)
}

fn recency_factor(published_day: i64, today: i64) -> f64 {
    let days = (today - published_day).max(0) as f64;
    if days <= 1.0 {
        1.0
    } else if days <= 7.0 {
        0.8
    } else if days <= 30.0 {
        0.5
    } else if days <= 365.0 {
        0.25
    } else {
        0.1
    }
}

/// Parse common RSS/ISO date formats. Best effort; falls back to "today".
fn parse_rss_date(s: &Option<String>) -> Option<i64> {
    let s = s.as_ref()?.trim();
    // ISO 8601: "2026-08-12T10:00:00Z" or "2026-08-12"
    if let Some(t) = s.find('T') {
        let date_part = &s[..t];
        let parts: Vec<&str> = date_part.split('-').collect();
        if parts.len() == 3 {
            let y = parts[0].parse::<i32>().ok()?;
            let m = parts[1].parse::<u8>().ok()?;
            let d = parts[2].parse::<u8>().ok()?;
            if (1..=crate::PRESENT_YEAR + 1).contains(&y) {
                return Some(SimDate::from_ce(y, m, d).days_from_ce());
            }
        }
    }
    // RFC 822: "Wed, 12 Aug 2026 10:00:00 GMT"
    let tokens: Vec<&str> = s.split([' ', ',']).filter(|t| !t.is_empty()).collect();
    let months = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    for (i, tok) in tokens.iter().enumerate() {
        if let Some(mi) = months.iter().position(|m| tok.eq_ignore_ascii_case(m)) {
            let day = tokens.get(i.saturating_sub(1)).and_then(|d| d.parse::<u8>().ok())?;
            let year = tokens.get(i + 1).and_then(|y| y.parse::<i32>().ok())?;
            if (1..=crate::PRESENT_YEAR + 1).contains(&year) {
                return Some(SimDate::from_ce(year, mi as u8 + 1, day).days_from_ce());
            }
        }
    }
    None
}

fn stable_hash(s: &str) -> u64 {
    let mut h = 5381u64;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u64);
    }
    h
}

/// Convert the top unprocessed news items into NewsSeed events dated at
/// `seed_date`, storing them as scenario events of `scenario_id`/`branch_id`.
/// Marks items processed on success.
pub fn seed_from_news(
    storage: &Storage,
    scenario_id: &str,
    branch_id: &str,
    seed_date: SimDate,
    limit: usize,
) -> Result<usize> {
    use crate::events::{EventPayload, HistoryEvent, NewsSeed};
    let items = storage.top_news_items(limit)?;
    let mut seq = storage.last_scenario_seq(scenario_id, branch_id)?;
    let mut stored = 0usize;
    for item in items {
        let ev = HistoryEvent {
            id: 0,
            date: SimDate::from_days(item.published_day).max(seed_date),
            scenario_id: Some(scenario_id.to_string()),
            title: item.title.clone(),
            body: item.summary.clone(),
            payload: EventPayload::NewsSeed(NewsSeed {
                headline: item.title,
                source: item.source_id,
                url: item.link,
                published: SimDate::from_days(item.published_day),
                confidence: item.confidence,
                nation: None,
            }),
            source_model: "news".into(),
            causal_parents: vec![],
            seq,
        };
        seq += 1;
        storage.add_scenario_event(&ev, branch_id)?;
        storage.mark_news_processed(&item.id)?;
        stored += 1;
    }
    Ok(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_dates() {
        assert_eq!(
            parse_rss_date(&Some("Wed, 12 Aug 2026 10:00:00 GMT".into())),
            Some(SimDate::from_ce(2026, 8, 12).days_from_ce())
        );
        assert_eq!(
            parse_rss_date(&Some("2026-08-12T10:00:00Z".into())),
            Some(SimDate::from_ce(2026, 8, 12).days_from_ce())
        );
    }

    #[test]
    fn trust_decays_with_age() {
        let today = SimDate::from_ce(2026, 8, 12).days_from_ce();
        let old = SimDate::from_ce(2025, 1, 1).days_from_ce();
        assert!(trust_score(0.9, today, today) > trust_score(0.9, old, today));
    }
}
