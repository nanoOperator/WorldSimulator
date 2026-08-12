export default function ProgressBox({ progress }) {
  const pct = progress ? Math.round(progress.percent * 100) : 0;
  return (
    <div className="progress card">
      <h2>Progress {progress ? "" : "(idle)"}</h2>
      <div className="bar"><div style={{ width: `${pct}%` }} /></div>
      <div className="msg">
        {progress
          ? `${progress.stage} — ${progress.message}`
          : "No simulation running."}
      </div>
      {progress && progress.log && progress.log.length > 0 && (
        <div className="feed" style={{ marginTop: 8 }}>
          {progress.log.slice(-6).reverse().map((l, i) => (
            <div className="item" key={i}>{l}</div>
          ))}
        </div>
      )}
    </div>
  );
}
