export default function ScenarioPanel({ scenarios, activeId, onSelect, onDelete }) {
  return (
    <div className="card">
      <h2>Scenarios</h2>
      {scenarios.length === 0 && <div className="meta">No scenarios yet. Create one from the prompt.</div>}
      {scenarios.map((s) => (
        <div
          key={s.id}
          className={`scenario-item ${s.id === activeId ? "active" : ""}`}
          onClick={() => onSelect(s)}
        >
          <div className="title">{s.title}</div>
          <div className="meta">
            created {s.created_at} · {s.branches ?? 0} branch(es)
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
