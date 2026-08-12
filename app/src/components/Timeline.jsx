import { useEffect, useRef } from "react";

// Auto-scrolling timeline of era markers across the full history span.
// Clicking a marker jumps the map to that era (calls onSeek with a BCE/CE year).
export default function Timeline({ eras, currentYear, onSeek }) {
  const ref = useRef(null);

  const minYear = -2000000;
  const maxYear = 2026;
  // Log-ish scale so Paleolithic doesn't get crushed to the left edge.
  const pos = (y) => {
    const clamped = Math.max(minYear, Math.min(maxYear, y));
    const t = Math.log10((clamped - minYear) + 1) / Math.log10((maxYear - minYear) + 1);
    return `${t * 100}%`;
  };

  return (
    <div className="timeline" ref={ref}>
      <div style={{ padding: "8px 14px", color: "var(--muted)", fontSize: 12 }}>
        <b style={{ color: "var(--text)" }}>Timeline</b> — scroll through 2 million years of history.
        Current view: <b style={{ color: "var(--text)" }}>{fmtYear(currentYear)}</b>
      </div>
      <div style={{ position: "relative", height: 60, margin: "0 16px", borderBottom: "1px solid var(--border)" }}>
        <div style={{ position: "absolute", left: 0, right: 0, top: 30, height: 2, background: "var(--border)" }} />
        {eras.map((e) => (
          <div key={e.label} title={`${e.label} (~${fmtYear(e.year)})`}
            style={{ position: "absolute", left: pos(e.year), top: 18, transform: "translateX(-50%)" }}
            onClick={() => onSeek && onSeek(e.year)}
          >
            <div style={{ width: 1, height: 12, background: "var(--accent)", margin: "0 auto" }} />
            <div style={{ fontSize: 10, color: "var(--muted)", whiteSpace: "nowrap", marginTop: 2 }}>{e.short}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

function fmtYear(y) {
  if (y === 0) return "1 CE";
  return y < 0 ? `${-y} BCE` : `${y} CE`;
}
