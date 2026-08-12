export default function StatsPanel({ snapshot, comparison }) {
  if (!snapshot) return <div className="card"><h2>World</h2><div className="meta">No data.</div></div>;
  const nations = snapshot.nations || [];
  const totalPop = nations.reduce((a, n) => a + (n.population || 0), 0);
  return (
    <div className="card">
      <h2>World @ {snapshot.date ? fmtDate(snapshot.date) : "?"}</h2>
      <div className="row" style={{ marginBottom: 10 }}>
        <div><div style={{ fontSize: 18, fontWeight: 700 }}>{nations.length}</div><div className="meta">nations</div></div>
        <div><div style={{ fontSize: 18, fontWeight: 700 }}>{fmtPop(totalPop)}</div><div className="meta">population</div></div>
        <div><div style={{ fontSize: 18, fontWeight: 700 }}>{(snapshot.techs || []).length}</div><div className="meta">techs</div></div>
      </div>
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
      <div style={{ maxHeight: 200, overflow: "auto" }}>
        {nations.slice().sort((a, b) => (b.population || 0) - (a.population || 0)).map((n) => (
          <div className="nation-row" key={n.id}>
            <span className="dot" style={{ background: n.color }} />
            <span>{n.name}</span>
            <span className="pop">{fmtPop(n.population)}</span>
          </div>
        ))}
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
