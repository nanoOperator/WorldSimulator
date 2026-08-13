import { useEffect, useMemo, useState } from "react";
import { api } from "../api.js";

const CAT_LABEL = {
  agriculture: "🌾",
  writing: "✍️",
  transport: "🚢",
  metallurgy: "⚒️",
  military: "⚔️",
  engineering: "🏗️",
  science: "🔬",
  energy: "⚡",
  industry: "🏭",
  medical: "🩺",
  communication: "📡",
  economy: "💰",
  computing: "💻",
  space: "🚀",
  navigation: "🧭",
};

export default function TechBrowser({ onClose }) {
  const [techs, setTechs] = useState([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [cat, setCat] = useState("all");

  useEffect(() => {
    let cancelled = false;
    api
      .timeline("", "")
      .then((events) => {
        if (cancelled) return;
        const byId = new Map();
        for (const ev of events || []) {
          const p = ev.payload;
          if (p && p.kind === "epoch_baseline" && Array.isArray(p.techs)) {
            const eraYear = ev.date?.year;
            for (const t of p.techs) {
              byId.set(t.tech_id, { ...t, eraYear });
            }
          }
        }
        setTechs(
          [...byId.values()].sort(
            (a, b) => (a.invented?.year || 0) - (b.invented?.year || 0)
          )
        );
        setLoading(false);
      })
      .catch(() => setLoading(false));
    return () => {
      cancelled = true;
    };
  }, []);

  const categories = useMemo(
    () => [...new Set(techs.map((t) => t.category).filter(Boolean))].sort(),
    [techs]
  );

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return techs.filter(
      (t) =>
        (cat === "all" || t.category === cat) &&
        (!q || t.name.toLowerCase().includes(q) || t.category.toLowerCase().includes(q))
    );
  }, [techs, query, cat]);

  const adopted = filtered.filter((t) => (t.adoption || 0) >= 0.5).length;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal tech-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Technology</h2>
          <div className="logs-actions">
            <input
              className="tech-search"
              placeholder="Search techs…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <select value={cat} onChange={(e) => setCat(e.target.value)}>
              <option value="all">All categories</option>
              {categories.map((c) => (
                <option key={c} value={c}>{CAT_LABEL[c] || "📘"} {c}</option>
              ))}
            </select>
            <button className="secondary" onClick={onClose}>Close</button>
          </div>
        </div>
        {loading ? (
          <div className="meta">Loading technology…</div>
        ) : (
          <>
            <div className="meta tech-summary">
              {filtered.length} technologies shown · {adopted} widely adopted
              (≥ 50%)
            </div>
            <div className="tech-list">
              {filtered.length === 0 && (
                <div className="meta">No technologies match.</div>
              )}
              {filtered.map((t) => (
                <div className="tech-row" key={t.tech_id}>
                  <span className="tech-cat" title={t.category}>
                    {CAT_LABEL[t.category] || "📘"}
                  </span>
                  <div className="tech-main">
                    <div className="tech-name">
                      <b>{t.name}</b>
                      <span className="meta">{fmtYear(t.invented?.year)}</span>
                    </div>
                    <div className="tech-bar">
                      <div
                        className="tech-bar-fill"
                        style={{ width: `${Math.round((t.adoption || 0) * 100)}%` }}
                      />
                    </div>
                  </div>
                  <span className="tech-pct">{Math.round((t.adoption || 0) * 100)}%</span>
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function fmtYear(y) {
  if (y == null) return "—";
  if (y === 0) return "1 BCE";
  return y < 0 ? `${-y} BCE` : `${y} CE`;
}
