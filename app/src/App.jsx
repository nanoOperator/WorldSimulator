import { useEffect, useState, useCallback } from "react";
import { api } from "./api.js";
import { startTheme, navSound, toggleMute, isMuted } from "./audio.js";
import WorldMap from "./components/WorldMap.jsx";
import Timeline from "./components/Timeline.jsx";
import PromptBox from "./components/PromptBox.jsx";
import ProgressBox from "./components/ProgressBox.jsx";
import ScenarioPanel from "./components/ScenarioPanel.jsx";
import BranchTree from "./components/BranchTree.jsx";
import StatsPanel from "./components/StatsPanel.jsx";

const PALEO_MARKERS = [
  { label: "Fire", year: -1900000, short: "fire" },
  { label: "Out of Africa", year: -1700000, short: "OoA" },
  { label: "Sapiens", year: -300000, short: "sap" },
  { label: "Cave art", year: -45000, short: "art" },
  { label: "Neolithic", year: -10000, short: "neo" },
  { label: "Writing", year: -3200, short: "wr" },
  { label: "Rome", year: 1, short: "R" },
  { label: "Islam", year: 622, short: "Is" },
  { label: "Printing", year: 1439, short: "pr" },
  { label: "Columbus", year: 1492, short: "Co" },
  { label: "WWI", year: 1914, short: "W1" },
  { label: "WWII", year: 1939, short: "W2" },
  { label: "Moon", year: 1969, short: "Mn" },
  { label: "2020", year: 2020, short: "20" },
];

export default function App() {
  const [eras, setEras] = useState(PALEO_MARKERS);
  const [scenarios, setScenarios] = useState([]);
  const [activeId, setActiveId] = useState(null);
  const [branches, setBranches] = useState([]);
  const [activeBranch, setActiveBranch] = useState(null);
  const [geojson, setGeojson] = useState(null);
  const [snapshot, setSnapshot] = useState(null);
  const [comparison, setComparison] = useState(null);
  const [progress, setProgress] = useState(null);
  const [prompt, setPrompt] = useState("");
  const [busy, setBusy] = useState(false);
  const [news, setNews] = useState([]);
  const [muted, setMuted] = useState(isMuted());

  useEffect(() => {
    const kick = () => startTheme();
    window.addEventListener("pointerdown", kick, { once: true });
    window.addEventListener("keydown", kick, { once: true });
    return () => {
      window.removeEventListener("pointerdown", kick);
      window.removeEventListener("keydown", kick);
    };
  }, []);

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
    const w = await api.world(activeId || "", activeBranch || "");
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
  }, [activeId, activeBranch]);

  useEffect(() => { refreshScenarios(); api.status().then((s) => { if (s.eras) setEras(PALEO_MARKERS.concat(s.eras.map((e) => ({ label: e.label, year: e.year, short: String(e.year).slice(-2) })))); }); api.news().then(setNews).catch(() => {}); }, []);
  useEffect(() => { loadWorld(); }, [loadWorld, activeId, activeBranch]);

  const pollProgress = useCallback(() => {
    let cancelled = false;
    const tick = async () => {
      const st = await api.simulateStatus();
      setProgress(st);
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
        divergence: "1945-01-01",
      });
      id = sc.id;
      setActiveId(id);
    }
    await api.simulate({ scenario_id: id, target_date: "2100", branch_count: 3, force_fallback: false });
    pollProgress();
  }, [prompt, activeId, scenarios, pollProgress]);

  const onSelectScenario = useCallback(async (s) => {
    navSound("open");
    setActiveId(s.id);
    setPrompt(s.prompt || "");
    setActiveBranch(null);
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

  const onSeek = useCallback(async (year) => {
    navSound("click");
    if (!activeId) return;
    const w = await api.world(activeId, activeBranch);
    setGeojson(w.geojson);
    setSnapshot(w.snapshot);
  }, [activeId, activeBranch]);

  const onSelectTerritory = useCallback((props) => {
    navSound("click");
    console.log("selected", props?.ownerName, props?.name);
  }, []);

  const refreshNews = useCallback(async () => {
    navSound("click");
    await api.refreshNews();
    setNews(await api.news());
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
          onCreate={() => doSimulate()}
        />
        <BranchTree branches={branches} activeBranch={activeBranch} onSelect={onSelectBranch} />
      </div>

      <div className="map">
        <WorldMap geojson={geojson} selected={activeBranch} onSelect={onSelectTerritory} />
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
        <StatsPanel snapshot={snapshot} comparison={comparison} />
        <ProgressBox progress={progress} />
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

      <Timeline eras={eras} currentYear={snapshot?.date?.year || 2020} onSeek={onSeek} />
    </div>
  );
}
