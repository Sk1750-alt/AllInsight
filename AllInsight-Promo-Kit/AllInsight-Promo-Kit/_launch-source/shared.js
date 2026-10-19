/*
  Shared pieces for the launch film and posters: the brand fonts, the A·i
  mark, and a faithful dark-theme Overview window built from the app's own
  tokens (src/index.css). Every animated value is a function of `p`, so a
  frame renders the same way every time it is asked for.

  The numbers inside the window are sample values, as in the earlier kit.
*/

const BRAND = {
  graphite: "#0f1115",
  black: "#060708",
  slate: "#1b1f27",
  line: "#2a2f38",
  paper: "#f4f2ee",
  fog: "#9aa1ac",
  amber: "#f5a524",
};

const clamp = (v, a = 0, b = 1) => Math.min(b, Math.max(a, v));
const seg = (t, a, b) => clamp((t - a) / (b - a));
const easeOut = (x) => 1 - Math.pow(1 - x, 4);
const easeExpo = (x) => (x >= 1 ? 1 : 1 - Math.pow(2, -10 * x));
const easeInOut = (x) => (x < 0.5 ? 8 * x * x * x * x : 1 - Math.pow(-2 * x + 2, 4) / 2);
const lerp = (a, b, x) => a + (b - a) * x;

document.head.insertAdjacentHTML(
  "beforeend",
  `<style>
@font-face { font-family: "Sora"; src: url("fonts/sora-latin-wght-normal.woff2") format("woff2-variations"); font-weight: 100 800; }
@font-face { font-family: "Instrument Serif"; src: url("fonts/instrument-serif-latin-400-normal.woff2") format("woff2"); }
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
html, body { background: ${BRAND.black}; color: ${BRAND.paper}; font-family: "Sora", sans-serif; -webkit-font-smoothing: antialiased; overflow: hidden; }
.serif { font-family: "Instrument Serif", serif; font-weight: 400; }

/* ---------- App window (dark theme tokens from src/index.css) ---------- */
.aw { --canvas:#121314; --surface:#1a1b1d; --raised:#202124; --ln:#2a2c2f; --ln2:#3a3d41; --sidebar:#161719;
  --ink:#ecedee; --muted:#a0a4aa; --subtle:#6f737a; --accent:#4fb7c9; --ok:#5cc28a; --warn:#e0a24a;
  position:absolute; width:1360px; height:840px; border-radius:18px; overflow:hidden; background:var(--canvas);
  box-shadow: 0 0 0 1px rgba(255,255,255,.09), 0 2px 0 rgba(255,255,255,.04) inset, 0 60px 160px rgba(0,0,0,.75), 0 20px 50px rgba(0,0,0,.5);
  font-size:14px; color:var(--ink); display:grid; grid-template-rows:44px 1fr; }
.aw-title { display:flex; align-items:center; gap:10px; padding:0 18px; background:var(--sidebar); border-bottom:1px solid var(--ln); font-size:12.5px; color:var(--muted); }
.aw-title .ctl { margin-left:auto; display:flex; gap:22px; color:var(--subtle); font-size:13px; }
.aw-body { display:grid; grid-template-columns:232px 1fr; min-height:0; }
.aw-side { background:var(--sidebar); border-right:1px solid var(--ln); padding:18px 12px; display:flex; flex-direction:column; gap:2px; }
.aw-side .grp { font-size:10.5px; letter-spacing:.18em; text-transform:uppercase; color:var(--subtle); padding:16px 12px 6px; font-weight:600; }
.aw-side .it { padding:8px 12px; border-radius:8px; color:var(--muted); font-size:13.5px; display:flex; align-items:center; gap:10px; }
.aw-side .it i { width:5px; height:5px; border-radius:50%; background:currentColor; opacity:.35; display:inline-block; }
.aw-side .it.on i { background:#f5a524; opacity:1; }
.aw-side .it.on { background:#26282b; color:var(--ink); }
.aw-main { padding:28px 32px; display:flex; flex-direction:column; gap:20px; min-height:0; }
.aw-h { display:flex; align-items:baseline; justify-content:space-between; }
.aw-h h3 { font-weight:500; font-size:26px; letter-spacing:-.01em; }
.aw-h span { color:var(--subtle); font-size:12.5px; }
.panel { background:var(--surface); border:1px solid var(--ln); border-radius:12px; }
.lab { font-size:10.5px; font-weight:600; text-transform:uppercase; letter-spacing:.2em; color:var(--subtle); }
.aw-top { display:grid; grid-template-columns:300px 1fr; gap:20px; }
.score { padding:24px; display:flex; flex-direction:column; align-items:center; gap:14px; }
.score .rs { color:var(--muted); font-size:12px; text-align:center; line-height:1.55; }
.ring { position:relative; width:150px; height:150px; }
.ring svg { position:absolute; inset:0; transform:rotate(-90deg); }
.ring .num { position:absolute; inset:0; display:flex; flex-direction:column; align-items:center; justify-content:center; }
.ring .num b { font-family:"Instrument Serif",serif; font-weight:400; font-size:58px; line-height:1; }
.ring .num small { color:var(--ok); opacity:var(--good,1); font-size:12px; margin-top:4px; }
.vitals { display:grid; grid-template-columns:repeat(4,1fr); gap:16px; }
.vital { padding:18px; display:flex; flex-direction:column; gap:10px; }
.vital .v { font-size:30px; font-weight:400; letter-spacing:-.02em; }
.vital .d { font-size:12px; color:var(--muted); line-height:1.45; min-height:34px; }
.bar { height:5px; border-radius:3px; background:#2a2c2f; overflow:hidden; }
.bar > div { height:100%; border-radius:3px; }
.dotk { width:7px; height:7px; border-radius:50%; display:inline-block; margin-right:7px; vertical-align:1px; }
.aw-low { display:grid; grid-template-columns:1fr 1.15fr; gap:20px; flex:1; min-height:0; }
.recs { padding:20px 22px; display:flex; flex-direction:column; gap:4px; }
.rec { display:flex; align-items:center; gap:14px; padding:13px 0; border-bottom:1px solid var(--ln); }
.rec:last-child { border-bottom:0; }
.rec .t { flex:1; }
.rec .t b { display:block; font-weight:500; font-size:14px; }
.rec .t span { color:var(--muted); font-size:12px; }
.rec .go { font-size:12px; color:var(--accent); border:1px solid #2c4b51; padding:6px 12px; border-radius:999px; }
.tm { padding:20px 22px; display:flex; flex-direction:column; gap:14px; }
.tmg { flex:1; display:grid; gap:4px; grid-template-columns:3fr 2fr 1.4fr; grid-template-rows:1.3fr 1fr 0.8fr; }
.tile { border-radius:6px; padding:10px 12px; font-size:12px; display:flex; flex-direction:column; justify-content:flex-end; color:rgba(255,255,255,.88); }
.tile b { font-weight:500; font-size:13px; } .tile span { opacity:.65; font-size:11px; }
</style>`
);

/* The A·i mark. `draw` 0..1 draws the A, `stem` raises the i, `dot` drops the amber dot. */
function markSVG(id = "mk", stroke = BRAND.paper) {
  return `<svg id="${id}" viewBox="0 0 100 100" width="100%" height="100%" style="overflow:visible">
    <defs><radialGradient id="${id}-g"><stop offset="0" stop-color="${BRAND.amber}" stop-opacity=".55"/><stop offset="1" stop-color="${BRAND.amber}" stop-opacity="0"/></radialGradient></defs>
    <path class="mA" d="M12 84 L50 18 L88 84" fill="none" stroke="${stroke}" stroke-width="12" stroke-linecap="round" stroke-linejoin="round" pathLength="1" stroke-dasharray="1 1"/>
    <line class="mI" x1="50" y1="72" x2="50" y2="84" stroke="${stroke}" stroke-width="12" stroke-linecap="round"/>
    <circle class="mG" cx="50" cy="54" r="26" fill="url(#${id}-g)"/>
    <circle class="mD" cx="50" cy="54" r="7" fill="${BRAND.amber}"/>
  </svg>`;
}
function setMark(root, draw, stem, dot, glow = dot) {
  const A = root.querySelector(".mA"), I = root.querySelector(".mI"), D = root.querySelector(".mD"), G = root.querySelector(".mG");
  A.style.strokeDashoffset = 1 - draw;
  A.style.opacity = draw > 0.001 ? 1 : 0;
  I.setAttribute("y1", lerp(84, 72, stem));
  I.style.opacity = stem > 0.001 ? 1 : 0;
  D.setAttribute("cy", lerp(30, 54, easeOut(dot)));
  D.style.opacity = clamp(dot * 3);
  G.style.opacity = glow;
}

function appWindowHTML() {
  const nav = (l, on) => `<div class="it${on ? " on" : ""}"><i></i>${l}</div>`;
  const vital = (title, val, detail, color, key) => `<div class="panel vital">
      <div class="lab"><span class="dotk" style="background:${color}"></span>${title}</div>
      <div class="v" data-k="${key}">${val}</div><div class="d">${detail}</div>
      <div class="bar"><div data-bar="${key}" style="background:${color}"></div></div></div>`;
  return `<div class="aw">
    <div class="aw-title"><svg width="16" height="16" viewBox="0 0 100 100"><rect width="100" height="100" rx="22" fill="#1B1F27"/><g transform="translate(19 19) scale(.62)"><path d="M12 84 L50 18 L88 84" fill="none" stroke="#F4F2EE" stroke-width="12" stroke-linecap="round" stroke-linejoin="round"/><circle cx="50" cy="54" r="7" fill="#F5A524"/></g></svg>AllInsight<div class="ctl"><span>—</span><span>▢</span><span>✕</span></div></div>
    <div class="aw-body">
      <div class="aw-side">${nav("Overview", 1)}<div class="grp">Storage</div>${nav("Storage Map")}${nav("Large Files")}${nav("Duplicates")}${nav("Cleanup")}<div class="grp">System</div>${nav("Performance")}${nav("Processes")}${nav("Startup")}${nav("Battery")}<div class="grp">&nbsp;</div>${nav("Drive Health")}${nav("AllInsight AI")}</div>
      <div class="aw-main">
        <div class="aw-h"><h3>Overview</h3><span>Scanned just now · nothing left this device</span></div>
        <div class="aw-top">
          <div class="panel score"><div class="lab">Device health</div>
            <div class="ring"><svg viewBox="0 0 150 150"><circle cx="75" cy="75" r="66" fill="none" stroke="#2a2c2f" stroke-width="9"/><circle class="arc" cx="75" cy="75" r="66" fill="none" stroke="#5cc28a" stroke-width="9" stroke-linecap="round" pathLength="100" stroke-dasharray="0 100"/></svg>
              <div class="num"><b class="sc">0</b><small>Good</small></div></div>
            <div class="rs">Drives healthy · startup is light<br>14.7 GB can be cleaned safely</div></div>
          <div class="vitals">
            ${vital("Storage", "68%", "152 GB free on C:", "#4fb7c9", "st")}
            ${vital("Performance", "12%", "6.1 GB of 16 GB memory in use", "#4fb7c9", "pf")}
            ${vital("Drive health", "Healthy", "96% of rated life remaining", "#5cc28a", "dr")}
            ${vital("Battery", "87%", "94% of design capacity", "#5cc28a", "bt")}
          </div>
        </div>
        <div class="aw-low">
          <div class="panel recs"><div class="lab" style="margin-bottom:6px">Recommendations</div>
            <div class="rec"><div class="t"><b>14.7 GB of caches and temp files</b><span>Safe to clean · dry run first</span></div><div class="go">Review</div></div>
            <div class="rec"><div class="t"><b>3 sets of duplicate files</b><span>2.1 GB · matched by BLAKE3, never by name</span></div><div class="go">Compare</div></div>
            <div class="rec"><div class="t"><b>4 apps launch at sign-in</b><span>Two have not been opened in 90 days</span></div><div class="go">Open</div></div>
          </div>
          <div class="panel tm"><div class="lab">Storage map · C:</div>
            <div class="tmg">
              <div class="tile" style="grid-row:1/3;background:#1d4a52"><b>Users</b><span>142 GB</span></div>
              <div class="tile" style="background:#24585f"><b>Program Files</b><span>61 GB</span></div>
              <div class="tile" style="background:#2c3b3e"><b>Windows</b><span>38 GB</span></div>
              <div class="tile" style="grid-column:2/4;background:#3b3324"><b>Caches · safe</b><span>14.7 GB</span></div>
              <div class="tile" style="grid-column:1/2;background:#22302f"><b>Games</b><span>48 GB</span></div>
              <div class="tile" style="background:#262a2e"><b>Other</b><span>9 GB</span></div>
              <div class="tile" style="background:#2a2627"><b>Dupes</b><span>2.1 GB</span></div>
            </div></div>
        </div>
      </div>
    </div></div>`;
}

/* p: 0..1 — fills the score ring, the bars and the treemap tiles. */
function setAppWindow(root, p) {
  const ring = easeInOut(seg(p, 0.0, 0.7));
  root.querySelector(".arc").setAttribute("stroke-dasharray", `${92 * ring} 100`);
  root.querySelector(".sc").textContent = Math.round(92 * ring);
  root.querySelector(".arc").style.opacity = ring > 0.01 ? 1 : 0;
  root.querySelector(".ring").style.setProperty("--good", seg(p, 0.55, 0.75));
  const fills = { st: 68, pf: 12, dr: 96, bt: 87 };
  Object.entries(fills).forEach(([k, v], i) => {
    root.querySelector(`[data-bar="${k}"]`).style.width = `${v * easeOut(seg(p, 0.1 + i * 0.08, 0.6 + i * 0.08))}%`;
  });
  root.querySelectorAll(".tile").forEach((el, i) => {
    const q = easeOut(seg(p, 0.25 + i * 0.06, 0.6 + i * 0.06));
    el.style.opacity = q;
    el.style.transform = `scale(${lerp(0.94, 1, q)})`;
  });
  root.querySelectorAll(".rec").forEach((el, i) => {
    const q = easeOut(seg(p, 0.3 + i * 0.1, 0.7 + i * 0.1));
    el.style.opacity = q;
    el.style.transform = `translateY(${lerp(10, 0, q)}px)`;
  });
}

/* Static film grain, drawn once, offset per frame. */
const GRAIN = `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='240' height='240'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='3' stitchTiles='stitch'/%3E%3CfeColorMatrix values='0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 0.05 0'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E")`;
