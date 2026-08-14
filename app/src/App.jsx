import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import { api, isDesktop } from "./api.js";
import { startTheme, navSound, toggleMute, isMuted } from "./audio.js";
import WorldMap from "./components/WorldMap.jsx";
import Timeline from "./components/Timeline.jsx";
import PromptBox from "./components/PromptBox.jsx";
import ProgressBox from "./components/ProgressBox.jsx";
import ScenarioPanel from "./components/ScenarioPanel.jsx";
import BranchTree from "./components/BranchTree.jsx";
import StatsPanel from "./components/StatsPanel.jsx";
import SetupPanel from "./components/SetupPanel.jsx";
import LogsPanel from "./components/LogsPanel.jsx";
import TechBrowser from "./components/TechBrowser.jsx";

const PALEO_MARKERS = [
  { label: "Fire", year: -1900000, short: "Fire" },
  { label: "Out of Africa", year: -1700000, short: "OoA" },
  { label: "Sapiens", year: -300000, short: "Sapiens" },
  { label: "Cave art", year: -45000, short: "Art" },
  { label: "Neolithic", year: -10000, short: "Neolithic" },
  { label: "Writing", year: -3200, short: "Writing" },
  { label: "Rome", year: 1, short: "Rome" },
  { label: "Islam", year: 622, short: "Islam" },
  { label: "Printing", year: 1439, short: "Printing" },
  { label: "Columbus", year: 1492, short: "Columbus" },
  { label: "WWI", year: 1914, short: "WWI" },
  { label: "WWII", year: 1939, short: "WWII" },
  { label: "Moon", year: 1969, short: "Moon" },
  { label: "2020", year: 2020, short: "2020" },
];

const ERA_SHORT = {
  "Paleolithic / First Fire": "Fire",
  "Lower Paleolithic": "Lower",
  "Middle Paleolithic": "Middle",
  "Upper Paleolithic": "Upper",
  "Mesolithic / Neolithic": "Neolithic",
  "Bronze Age / Sumer": "Bronze",
  "Classical Antiquity": "Classical",
  "Middle Ages": "Medieval",
  "Renaissance / Age of Sail": "Renaissance",
  "Industrial Revolution": "Industrial",
  "Imperial Era": "Imperial",
  "World Wars": "World Wars",
  "Cold War": "Cold War",
  "Contemporary / Future": "Modern",
};
const shortFor = (name) => ERA_SHORT[name] || name.split("/")[0].trim();

export default function App() {
  const [eras, setEras] = useState(PALEO_MARKERS);
  const [scenarios, setScenarios] = useState([]);
  const [activeId, setActiveId] = useState(null);
  const [branches, setBranches] = useState([]);
  const [activeBranch, setActiveBranch] = useState(null);
  const [geojson, setGeojson] = useState(null);
  const [snapshot, setSnapshot] = useState(null);
  const [worldDate, setWorldDate] = useState(null);
  const [comparison, setComparison] = useState(null);
  const [mapLoading, setMapLoading] = useState(false);
  const [focusNation, setFocusNation] = useState("");
  const [divergence, setDivergence] = useState(1945);
  const [timelineMax, setTimelineMax] = useState(2026);
  const [progress, setProgress] = useState(null);
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const [news, setNews] = useState([]);
  const [muted, setMuted] = useState(isMuted());
  const [showSetup, setShowSetup] = useState(false);
  const [setup, setSetup] = useState(null);
  const [logs, setLogs] = useState([]);
  const [showLogs, setShowLogs] = useState(false);
  const [logAutoScroll, setLogAutoScroll] = useState(true);
  const [showTech, setShowTech] = useState(false);
  const lastLogRef = useRef("");

  // Fold the backend's capped log buffer into an unbounded session log. The
  // backend resets its buffer on each run and caps it at 200 lines, so we
  // diff from the last seen message (which also survives cap eviction).
  const mergeLogs = useCallback((st) => {
    const arr = st?.log || [];
    if (arr.length === 0) return;
    const lastSeen = lastLogRef.current;
    const idx = lastSeen ? arr.indexOf(lastSeen) : -1;
    const fresh = idx >= 0 ? arr.slice(idx + 1) : arr;
    if (fresh.length === 0) return;
    lastLogRef.current = arr[arr.length - 1];
    setLogs((prev) => [...prev, ...fresh.map((msg) => ({ msg, t: Date.now() }))]);
  }, []);

  useEffect(() => {
    if (!isDesktop) return;
    let cancelled = false;
    (async () => {
      try {
        await api.ensureSetup();
      } catch (e) {
        console.error("ensureSetup failed", e);
      }
      const tick = async () => {
        try {
          const s = await api.setupStatus();
          if (cancelled) return;
          setSetup(s);
          if (s.running) setTimeout(tick, 1500);
        } catch (e) {
          console.error("setupStatus failed", e);
        }
      };
      tick();
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const kick = () => startTheme();
    window.addEventListener("pointerdown", kick, { once: true });
    window.addEventListener("keydown", kick, { once: true });
    return () => {
      window.removeEventListener("pointerdown", kick);
      window.removeEventListener("keydown", kick);
    };
  }, []);

  useEffect(() => {
    if (!isDesktop) return;
    let unsub = () => {};
    api.onSimProgress((st) => {
      setProgress(st);
      mergeLogs(st);
    }).then((un) => {
      unsub = un;
    });
    return () => unsub();
  }, [mergeLogs]);

  const onToggleMute = useCallback(() => setMuted(toggleMute()), []);

  const refreshScenarios = useCallback(async () => {
    const list = await api.listScenarios();
    setScenarios(list);
    if (!activeId && list.length) {
      setActiveId(list[0].id);
      setPrompt(list[0].prompt || "");
    }
  }, [activeId]);

  const loadWorld = useCallback(async () => {
    setMapLoading(true);
    let w;
    try {
      w = await api.world(activeId || "", activeBranch || "", worldDate);
    } catch (e) {
      setMapLoading(false);
      return;
    }
    // Attach owner population to each polygon for the 2.5D extrusion.
    const popById = {};
    for (const n of w.snapshot?.nations || []) popById[n.id] = n.population || 0;
    if (w.geojson?.features) {
      for (const f of w.geojson.features) {
        f.properties.population = popById[f.properties.owner] || 0;
      }
    }
    setGeojson(w.geojson);
    setSnapshot(w.snapshot);
    setMapLoading(false);
    if (activeBranch) {
      try {
        const c = await api.compare(activeId, activeBranch);
        setComparison(c.comparison);
      } catch (e) {
        setComparison(null);
      }
    } else {
      setComparison(null);
    }
  }, [activeId, activeBranch, worldDate]);

  useEffect(() => {
    refreshScenarios();
    api.status()
      .then((s) => {
        if (s.eras && s.eras.length) {
          setEras(s.eras.map((e) => ({
            label: e.name,
            year: e.start?.year ?? e.year,
            short: shortFor(e.name),
          })));
        }
      })
      .catch(() => {});
    api.news().then(setNews).catch(() => {});
  }, []);
   useEffect(() => { loadWorld(); }, [loadWorld, activeId, activeBranch, worldDate]);

   const fmtYear = useCallback((y) => (y <= 0 ? `${1 - y} BCE` : `${y} CE`), []);

   // Divergence choices for new scenarios, derived from era baselines.
   const divergenceOptions = useMemo(() => {
     const seen = new Set();
     const out = [];
     for (const e of eras || []) {
       const y = e.year ?? e.start?.year;
       if (y == null || y < -3200 || seen.has(y)) continue;
       seen.add(y);
       out.push({ year: y, label: `${fmtYear(y)} — ${e.short || e.label || e.name}` });
     }
     if (!seen.has(1945)) out.push({ year: 1945, label: "1945 CE — WWII end" });
     return out.sort((a, b) => a.year - b.year);
   }, [eras, fmtYear]);

   // Only extend the timeline past the present if a simulation actually
   // produced events beyond it. Otherwise the seekable range is capped at today.
   const refreshTimelineMax = useCallback(async () => {
     let mx = 2026;
     try {
       const events = await api.timeline(activeId || "", activeBranch || "");
       for (const e of events || []) {
         const y = e.date?.year;
         if (y != null && y > mx) mx = y;
       }
     } catch {}
     setTimelineMax(mx);
   }, [activeId, activeBranch]);

   useEffect(() => { refreshTimelineMax(); }, [refreshTimelineMax]);


  const pollProgress = useCallback(() => {
    let cancelled = false;
    const tick = async () => {
      const st = await api.simulateStatus();
      setProgress(st);
      mergeLogs(st);
      if (st.running && !cancelled) {
        setTimeout(tick, 700);
      } else if (!st.running) {
        await refreshScenarios();
        const list = await api.listScenarios();
        const sc = list.find((x) => x.id === activeId) || list[0];
        if (sc) {
          setActiveId(sc.id);
          const br = await api.branches(sc.id);
          setBranches(br);
          if (br[0]) setActiveBranch(br[0].id);
        }
        setBusy(false);
      }
    };
    tick();
    return () => { cancelled = true; };
  }, [activeId, refreshScenarios]);

  const doSimulate = useCallback(async () => {
    if (!prompt.trim()) return;
    navSound("alert");
    setBusy(true);
    let id = activeId;
    if (!id || scenarios.find((s) => s.id === id)?.prompt !== prompt) {
      const sc = await api.createScenario({
        name: prompt.slice(0, 60),
        prompt,
        divergence: divergence < 0 ? `BCE ${-divergence}` : `${divergence}-01-01`,
      });
      id = sc.id;
      setActiveId(id);
    }
    await api.simulate({ scenario_id: id, target_date: "2100", branch_count: 3, force_fallback: false });
    pollProgress();
  }, [prompt, activeId, scenarios, pollProgress, divergence]);

  const onSelectScenario = useCallback(async (s) => {
    navSound("open");
    setActiveId(s.id);
    setPrompt(s.prompt || "");
    setFocusNation("");
    setDivergence(s.divergence?.year ?? 1945);
    setActiveBranch(null);
    setWorldDate(null);
    const br = await api.branches(s.id);
    setBranches(br);
    if (br[0]) setActiveBranch(br[0].id);
  }, []);

  const onSelectBranch = useCallback((bid) => { navSound("branch"); setActiveBranch(bid); }, []);

  const onDelete = useCallback(async (id) => {
    await api.deleteScenario(id);
    setActiveId(null);
    setBranches([]);
    setActiveBranch(null);
    refreshScenarios();
  }, [refreshScenarios]);

  const onSeek = useCallback((year) => {
    navSound("click");
    setWorldDate(year < 0 ? `BCE ${-year}` : `${year}-01-01`);
  }, []);

  const onSelectTerritory = useCallback((props) => {
    navSound("click");
    setFocusNation(props?.owner || "");
  }, []);

  const refreshNews = useCallback(async () => {
    navSound("click");
    try {
      await api.refreshNews();
    } catch {}
    // The server now returns immediately and fetches feeds in the background,
    // so poll until the first batch lands.
    const poll = async (left) => {
      try {
        const items = await api.news();
        if (items.length) {
          setNews(items);
          return;
        }
      } catch {}
      if (left > 0) setTimeout(() => poll(left - 1), 1500);
    };
    poll(6);
  }, []);

  return (
    <div className="app">
      <div className="topbar">
        <img src="/WorldSimulator.png" alt="WorldSimulator" className="logo" />
        <h1>WorldSimulator</h1>
        <span className="stat"><b>{scenarios.length}</b> scenarios</span>
        <span className="stat"><b>{snapshot ? (snapshot.nations || []).length : 0}</b> nations</span>
        <span className="spacer" />
        <button className="secondary audio-toggle" onClick={onToggleMute} title="Toggle sound">
          {muted ? "🔇 Muted" : "🔊 Sound"}
        </button>
        <button className="secondary" onClick={() => { navSound("click"); setShowSetup(true); }}>
          Engine setup
        </button>
        <button className="secondary logs-toggle" onClick={() => { navSound("click"); setShowLogs(true); }}>
          Logs{logs.length ? ` (${logs.length})` : ""}
        </button>
        <button className="secondary" onClick={() => { navSound("click"); setShowTech(true); }} title="Browse all technologies">
          Techs
        </button>
        <button className="secondary" onClick={refreshNews}>Refresh news</button>
      </div>

      <div className="pane left">
        <ScenarioPanel
          scenarios={scenarios}
          activeId={activeId}
          onSelect={onSelectScenario}
          onDelete={onDelete}
        />
        <PromptBox
          value={prompt}
          onChange={setPrompt}
          onSimulate={doSimulate}
          busy={busy}
          scenarios={scenarios}
          onCreate={onSelectScenario}
          divergence={divergence}
          onDivergence={setDivergence}
          divergenceOptions={divergenceOptions}
        />
        <BranchTree branches={branches} activeBranch={activeBranch} onSelect={onSelectBranch} />
      </div>

      <div className="map">
        <WorldMap
          geojson={geojson}
          selected={activeBranch}
          onSelect={onSelectTerritory}
          onJumpToFirst={() => onSeek(eras.length ? Math.min(...eras.map((e) => e.year)) : -3200)}
          focusId={focusNation}
        />
        {mapLoading && <div className="map-loading"><div className="spinner" /></div>}
        {snapshot && !mapLoading && (
          <div className="map-date">
            <span className="map-date-year">{fmtYear(snapshot.date?.year ?? 2026)}</span>
            {snapshot.date && (
              <span className="map-date-full">
                {snapshot.date.year}-{String(snapshot.date.month).padStart(2, "0")}-{String(snapshot.date.day).padStart(2, "0")}
              </span>
            )}
          </div>
        )}
        <div className="legend">
          {(snapshot?.nations || []).slice(0, 12).map((n) => (
            <div key={n.id}>
              <span className="swatch" style={{ background: n.color }} />
              {n.name}
            </div>
          ))}
        </div>
      </div>

      <div className="pane right">
        <StatsPanel snapshot={snapshot} comparison={comparison} focusId={focusNation} onFocus={setFocusNation} />
        <ProgressBox progress={progress} onOpenLogs={() => setShowLogs(true)} />
        <div className="card">
          <h2>News signals</h2>
          <div className="feed">
            {(news || []).slice(0, 8).map((n) => (
              <div className="item" key={n.id}>{n.title}</div>
            ))}
            {(!news || news.length === 0) && <div className="meta">No news yet. Refresh.</div>}
          </div>
        </div>
      </div>

      <Timeline eras={eras} currentYear={snapshot?.date?.year || 2026} onSeek={onSeek} maxYear={timelineMax} />
      {setup?.running && (
        <div className="setup-overlay">
          <div className="setup-card">
            <h2>Preparing WorldSimulator</h2>
            <p className="meta">{setup.message}</p>
            <div className="bar">
              <div
                className="bar-fill"
                style={{ width: Math.max(2, Math.round(setup.percent * 100)) + "%" }}
              />
            </div>
            <p className="meta">
              First-run setup downloads the local AI engine and models automatically. This can
              take a few minutes depending on your connection.
            </p>
          </div>
        </div>
      )}
      {showSetup && <SetupPanel onClose={() => setShowSetup(false)} />}
      {showLogs && (
        <LogsPanel
          logs={logs}
          autoScroll={logAutoScroll}
          onToggleAutoScroll={() => setLogAutoScroll((v) => !v)}
          onClear={() => { setLogs([]); lastLogRef.current = ""; }}
          onClose={() => setShowLogs(false)}
        />
      )}
      {showTech && <TechBrowser onClose={() => setShowTech(false)} />}
    </div>
  );
}
