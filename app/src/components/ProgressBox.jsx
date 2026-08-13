import { useEffect, useRef } from "react";

export default function ProgressBox({ progress, onOpenLogs }) {
  const logRef = useRef(null);
  const pct = progress ? Math.round(progress.percent * 100) : 0;
  const last = progress?.log?.length || 0;

  useEffect(() => {
    const el = logRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [last]);

  return (
    <div className="progress card">
      <div className="progress-head">
        <h2>
          Simulation{" "}
          {progress?.running
            ? "running"
            : progress?.log?.length
            ? "finished"
            : "(idle)"}
        </h2>
        <span className="progress-tools">
          {last > 0 && (
            <button className="secondary mini" onClick={onOpenLogs} title="Open live log">
              Log
            </button>
          )}
          {progress && <span className="progress-pct">{pct}%</span>}
        </span>
      </div>
      <div className="bar"><div style={{ width: `${pct}%` }} /></div>
      <div className="msg">
        {progress?.message || "No simulation running."}
      </div>
      {progress && last > 0 && (
        <div className="log-feed" ref={logRef}>
          {progress.log.map((l, i) => (
            <div
              key={i}
              className={i === last - 1 ? "log-line latest" : "log-line"}
            >
              <span className="log-bullet">{i === last - 1 ? "▸" : "·"}</span>
              {l}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
