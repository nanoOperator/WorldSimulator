import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const TRACK_MIN = -2000000;
const TRACK_MAX = 2100;

function makeZones(maxYear) {
  return [
    { min: -2000000, max: -50000, to: 0.12 }, // deep prehistory
    { min: -50000, max: -3000, to: 0.24 }, // later prehistory (sapiens → neolithic)
    { min: -3000, max: 1, to: 0.40 }, // ancient (writing → Rome)
    { min: 1, max: 1500, to: 0.62 }, // classical → medieval
    { min: 1500, max: Math.max(maxYear, 1500), to: 1.0 }, // modern + future
  ];
}

function fracForYear(y, zones, maxYear) {
  const clamped = Math.max(TRACK_MIN, Math.min(maxYear, y));
  let prevTo = 0;
  for (const z of zones) {
    if (clamped <= z.max) {
      const span = z.max - z.min;
      const t = span === 0 ? 0 : (clamped - z.min) / span;
      return prevTo + (z.to - prevTo) * t;
    }
    prevTo = z.to;
  }
  return 1.0;
}

function yearForFrac(t, zones, maxYear) {
  const f = Math.max(0, Math.min(1, t));
  let prevTo = 0;
  for (const z of zones) {
    if (f <= z.to) {
      const span = z.to - prevTo;
      const inner = span === 0 ? 0 : (f - prevTo) / span;
      return Math.round(z.min + (z.max - z.min) * inner);
    }
    prevTo = z.to;
  }
  return maxYear;
}

export default function Timeline({ eras, currentYear, onSeek, maxYear = TRACK_MAX }) {
  const zones = useMemo(() => makeZones(maxYear), [maxYear]);
  const trackRef = useRef(null);
  const dragging = useRef(false);
  const suppressClick = useRef(false);
  const debounceRef = useRef(null);
  const lastYearRef = useRef(currentYear);
  const [viewYear, setViewYear] = useState(currentYear);

  useEffect(() => {
    if (!dragging.current) setViewYear(currentYear);
  }, [currentYear]);

  useEffect(() => () => clearTimeout(debounceRef.current), []);

  const onWheel = (e) => {
    e.preventDefault();
    const cur = lastYearRef.current == null ? maxYear : lastYearRef.current;
    const step = Math.max(1, Math.abs(e.deltaY) / 100);
    const dir = e.deltaY > 0 ? 1 : -1;
    const scale = yearForFrac(fracForYear(cur, zones, maxYear) + 0.02, zones, maxYear) - yearForFrac(fracForYear(cur, zones, maxYear) - 0.02, zones, maxYear);
    const target = cur + dir * Math.max(step, scale * step * 2);
    seek(Math.round(target));
  };

  useEffect(() => {
    const el = trackRef.current;
    if (!el) return;
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
      seek(yearForFrac(t, zones, maxYear));
    },
    [seek, zones, maxYear]
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
    const cur = viewYear == null ? maxYear : viewYear;
    let next = null;
    if (e.key === "ArrowRight" || e.key === "PageDown") next = yearForFrac(fracForYear(cur, zones, maxYear) + (e.key === "ArrowRight" ? 0.01 : 0.1), zones, maxYear);
    else if (e.key === "ArrowLeft" || e.key === "PageUp") next = yearForFrac(fracForYear(cur, zones, maxYear) - (e.key === "ArrowLeft" ? 0.01 : 0.1), zones, maxYear);
    else if (e.key === "Home") next = TRACK_MIN;
    else if (e.key === "End") next = maxYear;
    if (next == null) return;
    e.preventDefault();
    seek(next);
    flush();
  };

  const onEraJump = (e) => {
    const val = e.target.value;
    if (val === "today") seek(maxYear);
    else if (val) seek(Number(val));
    flush();
  };

  const shown = viewYear == null ? maxYear : viewYear;
  const handleFrac = fracForYear(shown, zones, maxYear);

  const placed = [];
  const markers = eras.map((e) => ({ e, left: fracForYear(e.year, zones, maxYear) }));
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
          <option value="today">Today ({fmtYear(maxYear)})</option>
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
        aria-valuemax={maxYear}
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
