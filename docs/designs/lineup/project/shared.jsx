/* Shared: data, utility helpers, common subcomponents */

const { useState, useEffect, useRef, useMemo } = React;

/* ── Data ─────────────────────────────────────────────────── */
window.FESTS = [
  {
    id: "fieldday26",
    name: "Field Day",
    year: "2026",
    where: "Brockwell Park, London",
    city: "LONDON",
    cc: "UK",
    dates: "Aug 22 — 23",
    dateRange: ["AUG 22", "AUG 23"],
    daysAway: 4,
    stages: 6,
    sets: 64,
    saved: 12,
    hue: 1,
    genres: ["ELECTRONIC", "HOUSE"],
    status: "live",                  // live | upcoming | past
    headliners: ["Four Tet", "Bicep", "Floating Points"],
  },
  {
    id: "primavera26",
    name: "Primavera Pro",
    year: "2026",
    where: "Parc del Fòrum, Barcelona",
    city: "BARCELONA",
    cc: "ES",
    dates: "Jun 04 — 08",
    dateRange: ["JUN 04", "JUN 08"],
    daysAway: 32,
    stages: 9,
    sets: 142,
    saved: 24,
    hue: 2,
    genres: ["INDIE", "ELECTRONIC"],
    status: "upcoming",
    headliners: ["Charli XCX", "Mount Kimbie", "Yves Tumor"],
  },
  {
    id: "draaimolen",
    name: "Draaimolen",
    year: "2026",
    where: "Tilburg, NL",
    city: "TILBURG",
    cc: "NL",
    dates: "Sep 19 — 20",
    dateRange: ["SEP 19", "SEP 20"],
    daysAway: 119,
    stages: 4,
    sets: 38,
    saved: 0,
    hue: 3,
    genres: ["TECHNO"],
    status: "upcoming",
    headliners: ["DVS1", "Helena Hauff", "Stenny"],
  },
  {
    id: "houghton",
    name: "Houghton",
    year: "2026",
    where: "Houghton Hall, Norfolk",
    city: "NORFOLK",
    cc: "UK",
    dates: "Aug 06 — 10",
    dateRange: ["AUG 06", "AUG 10"],
    daysAway: 78,
    stages: 7,
    sets: 89,
    saved: 0,
    hue: 4,
    genres: ["ELECTRONIC", "24H"],
    status: "upcoming",
    headliners: ["Craig Richards", "Ben UFO", "Move D"],
  },
  {
    id: "dekmantel",
    name: "Dekmantel",
    year: "2026",
    where: "Amsterdamse Bos, NL",
    city: "AMSTERDAM",
    cc: "NL",
    dates: "Aug 05 — 09",
    dateRange: ["AUG 05", "AUG 09"],
    daysAway: 77,
    stages: 8,
    sets: 110,
    saved: 0,
    hue: 5,
    genres: ["HOUSE", "TECHNO"],
    status: "upcoming",
    headliners: ["Honey Dijon", "DJ Stingray", "Carista"],
  },
  {
    id: "berlinatonal",
    name: "Atonal",
    year: "2026",
    where: "Kraftwerk, Berlin",
    city: "BERLIN",
    cc: "DE",
    dates: "Aug 27 — 31",
    dateRange: ["AUG 27", "AUG 31"],
    daysAway: 99,
    stages: 5,
    sets: 72,
    saved: 0,
    hue: 1,
    genres: ["AMBIENT", "INDUSTRIAL"],
    status: "upcoming",
    headliners: ["Lyra Pramuk", "Ben Frost", "Pole"],
  },
  {
    id: "ade25",
    name: "ADE",
    year: "2025",
    where: "Amsterdam, NL",
    city: "AMSTERDAM",
    cc: "NL",
    dates: "Oct 16 — 20 · 2025",
    dateRange: ["OCT 16", "OCT 20"],
    daysAway: -218,
    stages: 0,
    sets: 0,
    saved: 7,
    hue: 2,
    genres: ["CONF", "ELECTRONIC"],
    status: "past",
    headliners: [],
  },
];

/* ── Helpers ──────────────────────────────────────────────── */
window.fmtCountdown = (days) => {
  if (days === 0) return "TODAY";
  if (days < 0) return `${Math.abs(days)}D AGO`;
  if (days < 10) return `T−${String(days).padStart(2, "0")}D`;
  return `T−${days}D`;
};

/* ── Icon — Lucide passthrough ────────────────────────────── */
function Icon({ name, size = 18, stroke = 1.5, color = "currentColor", className = "" }) {
  const ref = useRef(null);
  useEffect(() => {
    if (!ref.current || !window.lucide) return;
    ref.current.innerHTML = "";
    const lib = window.lucide.icons || window.lucide;
    const src = lib[name] || lib.Square;
    if (!src) return;
    const svg = window.lucide.createElement(src);
    svg.setAttribute("width", size);
    svg.setAttribute("height", size);
    svg.setAttribute("stroke", color);
    svg.setAttribute("stroke-width", stroke);
    svg.setAttribute("stroke-linecap", "square");
    svg.setAttribute("stroke-linejoin", "miter");
    ref.current.appendChild(svg);
  }, [name, size, stroke, color]);
  return <span ref={ref} className={"lc " + className} style={{ display: "inline-flex", alignItems: "center" }} />;
}

/* ── Status bar ───────────────────────────────────────────── */
function StatusBar({ time = "20:30", carrier = "OFFBEAT", battery = "87%" }) {
  return (
    <div className="statusbar">
      <span>{time}</span>
      <span className="right">
        <span>●●●</span>
        <span>{carrier}</span>
        <span>{battery}</span>
      </span>
    </div>
  );
}

/* ── Mark (3-bar equalizer) ───────────────────────────────── */
function Mark() {
  return (
    <span className="mark-svg" aria-label="OFFBEAT mark">
      <span></span><span></span><span></span>
    </span>
  );
}

/* ── Duotone tile ─────────────────────────────────────────── */
function FestArt({ hue, w = 80, h = 80, label, children, style = {} }) {
  return (
    <div className="fest-art" data-hue={hue} style={{ width: w, height: h, ...style }}>
      <div className="grain"></div>
      {label && (
        <span style={{
          position: "absolute", bottom: 6, left: 8,
          fontFamily: "var(--font-mono)", fontSize: 9, color: "rgba(255,255,255,0.7)",
          textTransform: "uppercase", letterSpacing: "0.1em",
        }}>{label}</span>
      )}
      {children}
    </div>
  );
}

Object.assign(window, { Icon, StatusBar, Mark, FestArt });
