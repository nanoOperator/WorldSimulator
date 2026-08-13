// Self-contained audio: looping theme music + synthesized navigation blips,
// unified under a single master gain so one mute toggle controls everything.
// Browsers block audio until a user gesture, so startTheme() must be invoked
// from a click/keypress.

const THEME_URL = "/music/theme.mp3";
const STORE_KEY = "worldsim-audio-muted";

let ctx = null;
let master = null;
let themeEl = null;
let themeStarted = false;
let muted = localStorage.getItem(STORE_KEY) === "1";

function ensure() {
  if (ctx) return;
  const AC = window.AudioContext || window.webkitAudioContext;
  if (!AC) return;
  ctx = new AC();
  master = ctx.createGain();
  master.gain.value = muted ? 0 : 1;
  master.connect(ctx.destination);

  themeEl = new Audio(THEME_URL);
  themeEl.loop = true;
  themeEl.volume = 0.5;
  const src = ctx.createMediaElementSource(themeEl);
  src.connect(master);
}

export function isMuted() {
  return muted;
}

export function toggleMute() {
  muted = !muted;
  localStorage.setItem(STORE_KEY, muted ? "1" : "0");
  if (master) master.gain.value = muted ? 0 : 1;
  return muted;
}

// Start the theme on the first user gesture (autoplay policy).
export function startTheme() {
  ensure();
  if (!ctx || themeStarted) return;
  themeStarted = true;
  if (ctx.state === "suspended") ctx.resume();
  themeEl.play().catch(() => {});
}

// Short navigation blip synthesized on the fly (no asset needed).
export function navSound(kind = "click") {
  ensure();
  if (!ctx) return;
  if (ctx.state === "suspended") ctx.resume();
  const now = ctx.currentTime;
  const osc = ctx.createOscillator();
  const g = ctx.createGain();
  const presets = {
    click: { freq: 660, type: "triangle", dur: 0.07 },
    open: { freq: 520, type: "sine", dur: 0.12 },
    branch: { freq: 440, type: "square", dur: 0.1 },
    alert: { freq: 880, type: "sawtooth", dur: 0.14 },
  };
  const p = presets[kind] || presets.click;
  osc.type = p.type;
  osc.frequency.setValueAtTime(p.freq, now);
  osc.frequency.exponentialRampToValueAtTime(p.freq * 1.5, now + p.dur);
  g.gain.setValueAtTime(0.0001, now);
  g.gain.exponentialRampToValueAtTime(0.25, now + 0.01);
  g.gain.exponentialRampToValueAtTime(0.0001, now + p.dur);
  osc.connect(g);
  g.connect(master);
  osc.start(now);
  osc.stop(now + p.dur + 0.02);
}
