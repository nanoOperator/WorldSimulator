export default function PromptBox({ value, onChange, onSimulate, busy, scenarios, onCreate, divergence, onDivergence, divergenceOptions }) {
  return (
    <div className="card">
      <h2>Divergence prompt</h2>
      <textarea
        placeholder="e.g. What if the Nazis won WWII? What if Rome never fell?"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      <div className="divergence-row">
        <label htmlFor="divergence">Diverge at</label>
        <select
          id="divergence"
          value={divergence}
          onChange={(e) => onDivergence(Number(e.target.value))}
          title="Year the alternate timeline branches from real history"
        >
          {(divergenceOptions || []).map((o) => (
            <option key={o.year} value={o.year}>{o.label}</option>
          ))}
        </select>
      </div>
      <div className="row" style={{ marginTop: 8 }}>
        <select defaultValue="" onChange={(e) => {
          if (!e.target.value) return;
          const sid = e.target.value;
          const sc = scenarios.find((s) => s.id === sid);
          if (sc) {
            e.target.value = "";
            onCreate(sc);
          }
        }}>
          <option value="">+ New scenario from prompt…</option>
          {scenarios.map((s) => (
            <option key={s.id} value={s.id}>{s.name}</option>
          ))}
        </select>
      </div>
      <button style={{ marginTop: 8, width: "100%" }} disabled={busy} onClick={onSimulate}>
        {busy ? "Simulating…" : "Run simulation"}
      </button>
    </div>
  );
}
