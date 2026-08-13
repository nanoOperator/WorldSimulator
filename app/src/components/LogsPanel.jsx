import { useEffect, useRef } from "react";

function fmtTime(t) {
  const d = new Date(t);
  const p = (n) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

export default function LogsPanel({ logs, autoScroll, onToggleAutoScroll, onClear, onClose }) {
  const feedRef = useRef(null);

  useEffect(() => {
    const el = feedRef.current;
    if (el && autoScroll) el.scrollTop = el.scrollHeight;
  }, [logs.length, autoScroll]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal logs-modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Live simulation log</h2>
          <div className="logs-actions">
            <label className="logs-autoscroll">
              <input type="checkbox" checked={autoScroll} onChange={onToggleAutoScroll} />
              auto-scroll
            </label>
            <button className="secondary" onClick={onClear} title="Clear log">Clear</button>
            <button className="secondary" onClick={onClose}>Close</button>
          </div>
        </div>
        <div className="logs-feed" ref={feedRef}>
          {logs.length === 0 && <div className="meta">No log lines yet. Start a simulation to see live progress.</div>}
          {logs.map((l, i) => (
            <div key={i} className="logs-line">
              <span className="logs-time">{fmtTime(l.t)}</span>
              <span className="logs-msg">{l.msg}</span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
