import { useEffect, useState } from "react";
import { api, isDesktop } from "../api.js";

function fmtBytes(n) {
  if (!n) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(n) / Math.log(1024));
  return (n / Math.pow(1024, i)).toFixed(i ? 1 : 0) + " " + u[i];
}

export default function SetupPanel({ onClose }) {
  const [status, setStatus] = useState(null);
  const [progress, setProgress] = useState(null);
  const [busy, setBusy] = useState(false);
  const [url, setUrl] = useState("");
  const [filename, setFilename] = useState("");
  const [log, setLog] = useState([]);

  const refresh = async () => {
    try {
      setStatus(await api.engineStatus());
    } catch (e) {
      setLog((l) => [...l, "status error: " + e]);
    }
  };

  useEffect(() => {
    if (isDesktop) refresh();
  }, []);

  useEffect(() => {
    if (!isDesktop) return;
    let unsub = () => {};
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const a = await listen("engine-progress", (e) => {
        const p = e.payload;
        setProgress({ ...p, kind: "engine" });
        if (p.message) setLog((l) => [...l, `[engine] ${p.message}`]);
      });
      const b = await listen("model-progress", (e) => {
        setProgress({ ...e.payload, kind: "model" });
      });
      unsub = () => {
        a();
        b();
      };
    })();
    return () => unsub();
  }, []);

  const setupEngine = async () => {
    setBusy(true);
    setLog((l) => [...l, "Downloading llama.cpp engine..."]);
    try {
      const r = await api.setupEngine(true);
      setLog((l) => [...l, "engine: " + JSON.stringify(r)]);
    } catch (e) {
      setLog((l) => [...l, "engine error: " + e]);
    }
    setBusy(false);
    refresh();
  };

  const dlPreset = async (p) => {
    setBusy(true);
    setLog((l) => [...l, `Downloading ${p.name}...`]);
    try {
      const r = await api.downloadModel(p.url, p.filename, true);
      setLog((l) => [...l, "model: " + JSON.stringify(r)]);
    } catch (e) {
      setLog((l) => [...l, "model error: " + e]);
    }
    setBusy(false);
    refresh();
  };

  const dlCustom = async () => {
    if (!url) return;
    setBusy(true);
    try {
      const r = await api.downloadModel(url, filename || undefined, true);
      setLog((l) => [...l, "model: " + JSON.stringify(r)]);
    } catch (e) {
      setLog((l) => [...l, "model error: " + e]);
    }
    setBusy(false);
    refresh();
  };

  const pct = (p) => (p && p.percent >= 0 ? Math.round(p.percent * 100) : null);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Engine setup</h2>
          <button className="secondary" onClick={onClose}>
            Close
          </button>
        </div>
        {!isDesktop && (
          <div className="meta">
            Run inside the desktop app to download the local LLM engine.
          </div>
        )}
        {isDesktop && (
          <>
            <div className="card">
              <h3>llama.cpp engine</h3>
              <div className="meta">
                {status?.engine_installed ? "✅ Installed" : "❌ Not installed"} —{" "}
                {status?.bin_dir}
              </div>
              <button className="primary" disabled={busy} onClick={setupEngine}>
                Download &amp; install engine
              </button>
            </div>
            <div className="card">
              <h3>Models</h3>
              {(status?.models || []).map((m) => (
                <div key={m.id} className="model-row">
                  <div>
                    <b>{m.name}</b>{" "}
                    <span className="meta">
                      {m.present ? `✅ ${fmtBytes(m.size)}` : "not downloaded"}
                    </span>
                  </div>
                  <button
                    className="secondary"
                    disabled={busy || m.present}
                    onClick={() =>
                      dlPreset(status.presets.find((p) => p.id === m.id))
                    }
                  >
                    Download
                  </button>
                </div>
              ))}
            </div>
            <div className="card">
              <h3>Custom GGUF</h3>
              <input
                placeholder="https://...gguf"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
              />
              <input
                placeholder="filename (optional)"
                value={filename}
                onChange={(e) => setFilename(e.target.value)}
              />
              <button className="secondary" disabled={busy} onClick={dlCustom}>
                Download
              </button>
            </div>
            {progress && (
              <div className="card">
                <h3>Progress {progress.kind}</h3>
                <div className="bar">
                  <div
                    className="bar-fill"
                    style={{ width: (pct(progress) || 0) + "%" }}
                  />
                </div>
                <div className="meta">
                  {pct(progress) !== null ? pct(progress) + "%" : ""}{" "}
                  {progress.downloaded ? fmtBytes(progress.downloaded) : ""}{" "}
                  {progress.total ? "/ " + fmtBytes(progress.total) : ""}
                </div>
              </div>
            )}
            <div className="card log">
              {log.slice(-12).map((l, i) => (
                <div key={i} className="meta">
                  {l}
                </div>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
