import { useMemo, useState } from "react";

export default function StatsPanel({ snapshot, comparison }) {
  const [query, setQuery] = useState("");
  const [focusId, setFocusId] = useState("");

  const nations = snapshot ? snapshot.nations || [] : [];
  const totalPop = nations.reduce((a, n) => a + (n.population || 0), 0);

  const sorted = useMemo(
    () => nations.slice().sort((a, b) => (b.population || 0) - (a.population || 0)),
    [nations]
  );

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return sorted;
    return sorted.filter((n) => n.name.toLowerCase().includes(q));
  }, [sorted, query]);

  const focus = focusId
    ? nations.find((n) => n.id === focusId)
    : query.trim()
    ? shown[0]
    : null;

  if (!snapshot) return <div className="card"><h2>World</h2><div className="meta">No data.</div></div>;

  return (
    <div className="card">
      <h2>World @ {snapshot.date ? fmtDate(snapshot.date) : "?"}</h2>
      <div className="row" style={{ marginBottom: 10 }}>
        <div><div style={{ fontSize: 18, fontWeight: 700 }}>{nations.length}</div><div className="meta">nations</div></div>
        <div><div style={{ fontSize: 18, fontWeight: 700 }}>{fmtPop(totalPop)}</div><div className="meta">population</div></div>
        <div><div style={{ fontSize: 18, fontWeight: 700 }}>{(snapshot.techs || []).length}</div><div className="meta">techs</div></div>
      </div>

      <div className="nation-controls">
        <select value={focusId} onChange={(e) => { setFocusId(e.target.value); setQuery(""); }}>
          <option value="">Select a country…</option>
          {nations.slice().sort((a, b) => a.name.localeCompare(b.name)).map((n) => (
            <option key={n.id} value={n.id}>{n.name}</option>
          ))}
        </select>
        <input
          placeholder="Search countries…"
          value={query}
          onChange={(e) => { setQuery(e.target.value); setFocusId(""); }}
        />
      </div>

      {focus && (
        <div className="nation-detail">
          <div className="nation-detail-head">
            <span className="dot" style={{ background: focus.color }} />
            <b>{focus.name}</b>
            <span className="meta">{fmtPop(focus.population)}</span>
          </div>
          <div className="nation-detail-grid">
            <div>
              <div className="meta">Economy</div>
              <div className="index-bar"><div style={{ width: `${Math.min(100, focus.economy_index || 0)}%` }} /></div>
            </div>
            <div>
              <div className="meta">Military</div>
              <div className="index-bar"><div style={{ width: `${Math.min(100, focus.military_index || 0)}%` }} /></div>
            </div>
          </div>
          {(focus.religion_pct || []).filter(([, p]) => p > 0).length > 0 && (
            <div className="meta" style={{ marginTop: 6 }}>
              {focus.religion_pct.map(([r, p]) => `${r} ${p}%`).join(" · ")}
            </div>
          )}
          {(focus.ethnicity_pct || []).filter(([, p]) => p > 0).length > 0 && (
            <div className="meta" style={{ marginTop: 3 }}>
              {focus.ethnicity_pct.map(([e, p]) => `${e} ${p}%`).join(" · ")}
            </div>
          )}
        </div>
      )}

      {comparison && (
        <div className="feed" style={{ marginBottom: 8 }}>
          <div className="item"><b>Δ vs canonical</b></div>
          {comparison.changes && comparison.changes.slice(0, 8).map((c, i) => (
            <div className="item" key={i}>{c}</div>
          ))}
          {(!comparison.changes || comparison.changes.length === 0) && (
            <div className="item">no divergence detected</div>
          )}
        </div>
      )}

      <div className="nation-list">
        {shown.map((n) => (
          <div
            className={`nation-row${n.id === focusId ? " active" : ""}`}
            key={n.id}
            onClick={() => setFocusId(n.id)}
          >
            <span className="dot" style={{ background: n.color }} />
            <span className="nation-name">{n.name}</span>
            <span className="pop">{fmtPop(n.population)}</span>
          </div>
        ))}
        {shown.length === 0 && <div className="meta">No matching countries.</div>}
      </div>
    </div>
  );
}

function fmtPop(p) {
  if (!p) return "0";
  if (p >= 1e9) return `${(p / 1e9).toFixed(1)}B`;
  if (p >= 1e6) return `${(p / 1e6).toFixed(1)}M`;
  if (p >= 1e3) return `${(p / 1e3).toFixed(0)}K`;
  return `${p}`;
}
function fmtDate(d) {
  const y = d.year;
  return y < 0 ? `${-y} BCE` : `${y} CE`;
}
