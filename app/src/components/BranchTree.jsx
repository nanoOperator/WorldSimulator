export default function BranchTree({ branches, activeBranch, onSelect }) {
  return (
    <div className="card">
      <h2>Branches</h2>
      {!branches || branches.length === 0 && <div className="meta">Run a simulation to spawn branches.</div>}
      {(branches || []).map((b) => (
        <div
          key={b.id}
          className={`branch-item ${b.id === activeBranch ? "active" : ""}`}
          onClick={() => onSelect(b.id)}
        >
          <div className="branch-item-top">
            <span className={`branch-dot ${b.status || "pending"}`} title={b.status || "pending"} />
            <div className="title">{b.label || b.id}</div>
          </div>
          <div className="meta">
            seed {b.seed} · events {b.event_count ?? 0}{b.final_date ? ` · ${b.final_date}` : ""}
          </div>
        </div>
      ))}
    </div>
  );
}
