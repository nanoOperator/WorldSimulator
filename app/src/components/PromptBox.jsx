export default function PromptBox({ value, onChange, onSimulate, busy, scenarios, onCreate }) {
  return (
    <div className="card">
      <h2>Divergence prompt</h2>
      <textarea
        placeholder="e.g. What if the Nazis won WWII? What if Rome never fell?"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
      <div className="row" style={{ marginTop: 8 }}>
        <select defaultValue="" onChange={(e) => e.target.value && onCreate(e.target.value)}>
          <option value="">+ New scenario from prompt…</option>
          {scenarios.map((s) => (
            <option key={s.id} value={s.id}>{s.title}</option>
          ))}
        </select>
      </div>
      <button style={{ marginTop: 8, width: "100%" }} disabled={busy} onClick={onSimulate}>
        {busy ? "Simulating…" : "Run simulation"}
      </button>
    </div>
  );
}
