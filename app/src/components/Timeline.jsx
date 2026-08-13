import { useCallback, useEffect, useRef, useState } from "react";

const TRACK_MIN = -2000000;
const TRACK_MAX = 2100;

// Piecewise scale: history is mapped to the track in zones so that recent,
// simulation-relevant history gets a readable share instead of being crushed
// into a sliver by a single log over 2 million years.
const ZONES = [
  { min: -2000000, max: -50000, to: 0.12 }, // deep prehistory
  { min: -50000, max: -3000, to: 0.24 }, // later prehistory (sapiens → neolithic)
  { min: -3000, max: 1, to: 0.40 }, // ancient (writing → Rome)
  { min: 1, max: 1500, to: 0.62 }, // classical → medieval
  { min: 1500, max: 2100, to: 1.0 }, // modern + future
];

function fracForYear(y) {
  const clamped = Math.max(TRACK_MIN, Math.min(TRACK_MAX, y));
  let prevTo = 0;
  for (const z of ZONES) {
    if (clamped <= z.max) {
      const span = z.max - z.min;
      const t = span === 0 ? 0 : (clamped - z.min) / span;
      return prevTo + (z.to - prevTo) * t;
    }
    prevTo = z.to;
  }
  return 1.0;
}

function yearForFrac(t) {
  const f = Math.max(0, Math.min(1, t));
  let prevTo = 0;
  for (const z of ZONES) {
    if (f <= z.to) {
      const span = z.to - prevTo;
      const inner = span === 0 ? 0 : (f - prevTo) / span;
      return Math.round(z.min + (z.max - z.min) * inner);
    }
    prevTo = z.to;
  }
  return TRACK_MAX;
}

export default function Timeline({ eras, currentYear, onSeek }) {
  const trackRef = useRef(null);
  const dragging = useRef(false);
  const suppressClick = useRef(false);
  const debounceRef = useRef(null);
  const lastYearRef = useRef(currentYear);
  const [viewYear, setViewYear] = useState(currentYear);

  // Follow external changes (scenario switch, seek from elsewhere).
  useEffect(() => {
    if (!dragging.current) setViewYear(currentYear);
  }, [currentYear]);

  useEffect(() => () => clearTimeout(debounceRef.current), []);

  // Wheel / trackpad scrolling over the track travels through history.
  useEffect(() => {
    const el = trackRef.current;
    if (!el) return;
    const onWheel = (e) => {
      e.preventDefault();
      const cur = lastYearRef.current == null ? TRACK_MAX : lastYearRef.current;
      const step = Math.max(1, Math.abs(e.deltaY) / 100);
      const dir = e.deltaY > 0 ? 1 : -1;
      // Constant ~1 year per "notch", scaled across the piecewise zones.
      const scale = yearForFrac(fracForYear(cur) + 0.02) - yearForFrac(fracForYear(cur) - 0.02);
      const target = cur + dir * Math.max(step, scale * step * 2);
      seek(Math.round(target));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  });

  const seek = useCallback(
    (y) => {
      lastYearRef.current = y;
      setViewYear(y);
      clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => onSeek && onSeek(y), 120);
    },
    [onSeek]
  );

  // Flush the pending debounced seek (drag/keyboard end, marker click).
  const flush = useCallback(() => {
    clearTimeout(debounceRef.current);
    if (onSeek) onSeek(lastYearRef.current);
  }, [onSeek]);

  const seekFromEvent = useCallback(
    (clientX) => {
      const el = trackRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      if (rect.width === 0) return;
      const t = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      seek(yearForFrac(t));
    },
    [seek]
  );

  const onPointerDown = (e) => {
    dragging.current = true;
    suppressClick.current = true;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    seekFromEvent(e.clientX);
  };

  const onPointerMove = (e) => {
    if (dragging.current) seekFromEvent(e.clientX);
  };

  const endDrag = (e) => {
    if (!dragging.current) return;
    dragging.current = false;
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    flush();
    setTimeout(() => (suppressClick.current = false), 0);
  };

  const onKeyDown = (e) => {
    const cur = viewYear == null ? TRACK_MAX : viewYear;
    let next = null;
    if (e.key === "ArrowRight" || e.key === "PageDown") next = yearForFrac(fracForYear(cur) + (e.key === "ArrowRight" ? 0.01 : 0.1));
    else if (e.key === "ArrowLeft" || e.key === "PageUp") next = yearForFrac(fracForYear(cur) - (e.key === "ArrowLeft" ? 0.01 : 0.1));
    else if (e.key === "Home") next = TRACK_MIN;
    else if (e.key === "End") next = TRACK_MAX;
    if (next == null) return;
    e.preventDefault();
    seek(next);
    flush();
  };

  const onEraJump = (e) => {
    const val = e.target.value;
    if (val === "today") seek(2020);
    else if (val) seek(Number(val));
    flush();
  };

  const shown = viewYear == null ? TRACK_MAX : viewYear;
  const handleFrac = fracForYear(shown);

  // Greedy label placement: drop labels that would collide with an earlier one.
  const placed = [];
  const markers = eras.map((e) => ({ e, left: fracForYear(e.year) }));
  for (const m of markers) {
    m.showLabel = placed.every((p) => Math.abs(m.left - p) >= 0.024);
    if (m.showLabel) placed.push(m.left);
  }

  return (
    <div className="timeline">
      <div className="tl-head">
        <b>Timeline</b>
        <span className="tl-current">Current view: {fmtYear(shown)}</span>
        <span className="tl-hint">drag, scroll, or use keys</span>
        <select className="tl-jump" value="" onChange={onEraJump} title="Jump to an era">
          <option value="" disabled>Jump to era…</option>
          <option value="today">Today (2020)</option>
          {eras.map((e) => (
            <option key={e.label} value={e.year}>≈ {fmtYear(e.year)} — {e.label}</option>
          ))}
        </select>
      </div>
      <div
        ref={trackRef}
        className="tl-track"
        tabIndex={0}
        role="slider"
        aria-label="Timeline"
        aria-valuemin={TRACK_MIN}
        aria-valuemax={TRACK_MAX}
        aria-valuenow={shown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onKeyDown={onKeyDown}
      >
        <div className="tl-line" />
        {markers.map(({ e, left, showLabel }) => (
          <div
            key={e.label}
            className="tl-marker"
            title={`${e.label} (~${fmtYear(e.year)})`}
            style={{ left: `${left * 100}%` }}
            onClick={() => {
              if (suppressClick.current) return;
              seek(e.year);
              flush();
            }}
          >
            <div className="tl-tick" />
            {showLabel && <div className="tl-label">{e.short}</div>}
          </div>
        ))}
        <div className="tl-handle" style={{ left: `${handleFrac * 100}%` }}>
          <div className="tl-bubble">{fmtYear(shown)}</div>
          <div className="tl-knob" />
        </div>
      </div>
    </div>
  );
}

function fmtYear(y) {
  if (y == null) return "—";
  if (y === 0) return "1 BCE";
  return y < 0 ? `${-y} BCE` : `${y} CE`;
}
