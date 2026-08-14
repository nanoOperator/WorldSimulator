function fmtDate(d) {
  if (!d) return "?";
  return d.year <= 0 ? `${1 - d.year} BCE` : `${d.year} CE`;
}

function relTime(iso) {
  if (!iso) return "";
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "";
  const s = Math.max(1, Math.floor((Date.now() - t) / 1000));
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

export default function ScenarioPanel({ scenarios, activeId, onSelect, onDelete }) {
  const sorted = [...scenarios].sort((a, b) => (b.created_at || "").localeCompare(a.created_at || ""));
  return (
    <div className="card">
      <h2>Scenarios</h2>
      {sorted.length === 0 && <div className="meta">No scenarios yet. Create one from the prompt.</div>}
      {sorted.map((s) => (
        <div
          key={s.id}
          className={`scenario-item ${s.id === activeId ? "active" : ""}`}
          onClick={() => onSelect(s)}
        >
          <div className="scenario-item-top">
            <div className="title">{s.name}</div>
            <span className="scenario-branches">{s.branches ?? 0} br</span>
          </div>
          <div className="meta">
            Δ {fmtDate(s.divergence)} · {relTime(s.created_at)}
          </div>
          {s.id === activeId && (
            <button className="secondary" style={{ marginTop: 6, fontSize: 11, padding: "4px 8px" }}
              onClick={(e) => { e.stopPropagation(); onDelete(s.id); }}>delete</button>
          )}
        </div>
      ))}
    </div>
  );
}
