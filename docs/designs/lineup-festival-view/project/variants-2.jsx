/* Festival Views — variants part 2 (filter panel, clash radar, now-strip)
   ──────────────────────────────────────────────────────────── */

/* ──────────────────────────────────────────────────────────────
   V4 — FILTER PANEL
   Bottom-sheet filter UI with chips, stage swatches, twin time
   range slider, and toggles. Active-filter pills shown on the
   summary bar above the list.
   ────────────────────────────────────────────────────────────── */
function V4Filters() {
  const [day] = useState("fri");
  const [open, setOpen] = useState(true);
  const [genres, setGenres] = useState(new Set(["TECHNO", "ELECTRONIC"]));
  const [stages, setStages] = useState(new Set(["s1", "s3"]));
  const [timeRange, setTimeRange] = useState([20 * 60, 26 * 60]); // 20:00–02:00
  const [starredOnly, setStarredOnly] = useState(false);
  const [hideClashes, setHideClashes] = useState(true);

  const toggle = (set, val) => {
    const n = new Set(set);
    n.has(val) ? n.delete(val) : n.add(val);
    return n;
  };

  const totalActive =
    genres.size + stages.size +
    (timeRange[0] !== 18 * 60 || timeRange[1] !== 26 * 60 ? 1 : 0) +
    (starredOnly ? 1 : 0) + (hideClashes ? 1 : 0);

  const clearAll = () => {
    setGenres(new Set());
    setStages(new Set());
    setTimeRange([18*60, 26*60]);
    setStarredOnly(false);
    setHideClashes(false);
  };

  const stageById = useMemo(() => {
    const m = {}; STAGES.forEach(s => m[s.id] = s); return m;
  }, []);

  // Time range slider math
  const RANGE_MIN = 18 * 60, RANGE_MAX = 26 * 60;
  const pctL = ((timeRange[0] - RANGE_MIN) / (RANGE_MAX - RANGE_MIN)) * 100;
  const pctR = ((timeRange[1] - RANGE_MIN) / (RANGE_MAX - RANGE_MIN)) * 100;

  // Mocked filtered list — just show all sets matching genre/stage for the preview
  const filtered = SETS.filter(s =>
    s.day === day &&
    (genres.size === 0 || genres.has(s.genre)) &&
    (stages.size === 0 || stages.has(s.stage)) &&
    s.t >= timeRange[0] && s.t + s.dur <= timeRange[1] + 30 &&
    (!starredOnly || s.starred) &&
    (!hideClashes || !(s.clashes && s.clashes.length))
  ).sort((a, b) => a.t - b.t);

  return (
    <div className="phone" data-screen-label="V4 Filters">
      <StatusBar />
      <TopNav
        festival="Field Day"
        right={
          <>
            <button className="icon-btn"><Icon name="Search" size={17} /></button>
            <button className="icon-btn" onClick={() => setOpen(o => !o)} style={{ position: "relative" }}>
              <Icon name="SlidersHorizontal" size={17} color={open ? "var(--accent)" : "currentColor"} />
              {totalActive > 0 && (
                <span style={{
                  position: "absolute", top: 4, right: 4,
                  width: 14, height: 14,
                  fontFamily: "var(--font-mono)", fontSize: 9, fontWeight: 700,
                  background: "var(--accent)", color: "var(--accent-ink)",
                  display: "flex", alignItems: "center", justifyContent: "center",
                }}>{totalActive}</span>
              )}
            </button>
          </>
        }
      />

      {/* Active filter summary bar */}
      <div className="filter-summary-bar">
        <button className="open-btn" onClick={() => setOpen(true)}>
          <Icon name="SlidersHorizontal" size={14} />
          Filters
          {totalActive > 0 && <span className="count">{totalActive}</span>}
        </button>
        {[...stages].map(sid => (
          <span key={sid} className="chip active" onClick={() => setStages(toggle(stages, sid))}>
            <span style={{ width: 8, height: 8, background: stageById[sid].color, display: "inline-block" }}></span>
            {stageById[sid].name} ×
          </span>
        ))}
        {[...genres].map(g => (
          <span key={g} className="chip active" onClick={() => setGenres(toggle(genres, g))}>
            {g} ×
          </span>
        ))}
        {hideClashes && (
          <span className="chip active" onClick={() => setHideClashes(false)}>NO CLASHES ×</span>
        )}
      </div>

      {/* Behind the panel: the list (for context) */}
      <div className="main" style={{ filter: open ? "blur(0.5px)" : "none", transition: "filter 200ms" }}>
        <div style={{
          padding: "10px 18px",
          fontFamily: "var(--font-mono)", fontSize: 10,
          textTransform: "uppercase", letterSpacing: "0.08em",
          color: "var(--fg-3)",
        }}>
          {filtered.length} SETS // FRI 22
        </div>
        {filtered.map(s => (
          <SetRow key={s.id} set={s} stage={stageById[s.stage]} onOpen={() => {}} onStar={() => {}} />
        ))}
        {filtered.length === 0 && (
          <div style={{
            padding: 30, textAlign: "center",
            color: "var(--fg-3)", fontFamily: "var(--font-mono)",
            fontSize: 11, textTransform: "uppercase", letterSpacing: "0.08em",
          }}>
            NO SETS // ADJUST FILTERS
          </div>
        )}
        <div style={{ height: 60 }}></div>
      </div>

      {/* Filter sheet */}
      {open && (
        <div className="fp-sheet">
          <div className="grip"></div>
          <div className="head">
            <h2 className="h2">Filters</h2>
            <button className="clear" onClick={clearAll}>CLEAR ALL ({totalActive})</button>
          </div>

          <div className="body">
            {/* STAGES */}
            <div className="fp-section">
              <div className="hdr">
                <span className="lbl">// STAGES</span>
                <span className="val">{stages.size || "ALL"}</span>
              </div>
              <div className="fp-stage-grid">
                {STAGES.map(s => (
                  <button
                    key={s.id}
                    className={"fp-stage-opt" + (stages.has(s.id) ? " on" : "")}
                    onClick={() => setStages(toggle(stages, s.id))}
                  >
                    <span className="sw" style={{ background: s.color }}></span>
                    <span className="nm">{s.name}</span>
                    <span className="ck">{stages.has(s.id) ? "●" : "○"}</span>
                  </button>
                ))}
              </div>
            </div>

            {/* TIME RANGE */}
            <div className="fp-section">
              <div className="hdr">
                <span className="lbl">// TIME WINDOW</span>
                <span className="val">{fmtTime(timeRange[0])} → {fmtTime(timeRange[1])}</span>
              </div>
              <div className="fp-range">
                <div className="track"></div>
                <div className="fill" style={{ left: pctL + "%", width: (pctR - pctL) + "%" }}></div>
                <div className="handle" style={{ left: pctL + "%" }}></div>
                <div className="handle" style={{ left: pctR + "%" }}></div>
                <div className="label" style={{ left: pctL + "%", top: -16 }}>{fmtTime(timeRange[0])}</div>
                <div className="label right" style={{ left: pctR + "%", top: -16 }}>{fmtTime(timeRange[1])}</div>
              </div>
              <div style={{ display: "flex", justifyContent: "space-between", marginTop: 14,
                fontFamily: "var(--font-mono)", fontSize: 9, color: "var(--fg-4)",
                letterSpacing: "0.05em",
              }}>
                <span>18:00</span><span>20:00</span><span>22:00</span><span>00:00</span><span>02:00</span>
              </div>
            </div>

            {/* GENRES */}
            <div className="fp-section">
              <div className="hdr">
                <span className="lbl">// GENRES</span>
                <span className="val">{genres.size || "ALL"}</span>
              </div>
              <div className="fp-chiprow">
                {GENRES.map(g => (
                  <button key={g}
                    className={"chip" + (genres.has(g) ? " active" : "")}
                    onClick={() => setGenres(toggle(genres, g))}
                  >{g}</button>
                ))}
              </div>
            </div>

            {/* TOGGLES */}
            <div className="fp-section">
              <div className="hdr">
                <span className="lbl">// SMART FILTERS</span>
              </div>
              <div className={"fp-toggle" + (starredOnly ? " on" : "")} onClick={() => setStarredOnly(v => !v)}>
                <div className="col">
                  <span className="lbl">★ Starred only</span>
                  <span className="sub">Show artists you've added</span>
                </div>
                <div className="sw"></div>
              </div>
              <div className="dotted-rule"></div>
              <div className={"fp-toggle" + (hideClashes ? " on" : "")} onClick={() => setHideClashes(v => !v)}>
                <div className="col">
                  <span className="lbl">× Hide clashing sets</span>
                  <span className="sub">Skip overlaps with your stars</span>
                </div>
                <div className="sw"></div>
              </div>
            </div>
          </div>

          <div className="fp-foot">
            <button className="btn-ghost" onClick={clearAll} style={{ flex: 1 }}>RESET</button>
            <button className="btn-primary" onClick={() => setOpen(false)} style={{ flex: 2 }}>
              SHOW {filtered.length} SETS →
            </button>
          </div>
        </div>
      )}

      <Caption
        name="V4 // FILTERS"
        desc="Stages as colored options, twin-handle time range, genre chips, smart toggles. Active filters bubble back up to a summary bar above the list."
      />
    </div>
  );
}

/* ──────────────────────────────────────────────────────────────
   V5 — CLASH RADAR
   Visual schedule overlap analysis. A compact stage-stacked
   strip with hatched warning zones where your starred sets
   overlap, plus a "resolve this" card per clash pair.
   ────────────────────────────────────────────────────────────── */
function V5ClashRadar() {
  const [day] = useState("fri");
  const stageById = useMemo(() => {
    const m = {}; STAGES.forEach(s => m[s.id] = s); return m;
  }, []);
  const sets = SETS.filter(s => s.day === day);
  const starred = sets.filter(s => s.starred);
  const clashSetIds = new Set();
  const clashPairs = [];
  starred.forEach(a => {
    starred.forEach(b => {
      if (a.id >= b.id) return;
      if (a.t < b.t + b.dur && b.t < a.t + a.dur) {
        clashSetIds.add(a.id);
        clashSetIds.add(b.id);
        clashPairs.push([a, b]);
      }
    });
  });

  // Window for the strip
  const W_START = 19 * 60;
  const W_END = 25 * 60;
  const W_RANGE = W_END - W_START;
  const xPct = m => Math.max(0, Math.min(100, ((m - W_START) / W_RANGE) * 100));

  // Show stages that actually have starred sets, then a few more
  const stagesWithStars = STAGES.filter(st => starred.some(s => s.stage === st.id));
  const otherStages = STAGES.filter(st => !stagesWithStars.includes(st)).slice(0, 0);
  const visibleStages = [...stagesWithStars, ...otherStages];

  // Build clash zone rects (continuous spans on a single combined timeline)
  const clashZones = clashPairs.map(([a, b]) => {
    const start = Math.max(a.t, b.t);
    const end   = Math.min(a.t + a.dur, b.t + b.dur);
    return { start, end, pair: [a, b] };
  });

  return (
    <div className="phone" data-screen-label="V5 Clash radar">
      <StatusBar />
      <TopNav festival="Field Day" right={<NavRightStandard />} showBack onBack={() => {}} />

      <div className="main">
        <div className="clash-hero">
          <div className="super">// YOUR PLAN · FRI 22 AUG</div>
          <h1>
            {clashPairs.length} <span className="acc">clash{clashPairs.length === 1 ? "" : "es"}</span> <br />
            in your night.
          </h1>
        </div>

        {/* Mini strip diagram */}
        <div className="clash-strip">
          <div className="axis">
            <span>{fmtTime(W_START)}</span>
            <span>{fmtTime(W_START + W_RANGE * 0.25)}</span>
            <span>{fmtTime(W_START + W_RANGE * 0.5)}</span>
            <span>{fmtTime(W_START + W_RANGE * 0.75)}</span>
            <span>{fmtTime(W_END)}</span>
          </div>
          <div className="lanes" style={{ paddingLeft: 32 }}>
            {visibleStages.map((stage, i) => {
              const stageSets = sets.filter(s => s.stage === stage.id);
              return (
                <div key={stage.id} className="lane">
                  <span className="lbl">{stage.short}</span>
                  {stageSets.map(s => {
                    const left = xPct(s.t);
                    const right = xPct(s.t + s.dur);
                    if (right < 0 || left > 100) return null;
                    return (
                      <div key={s.id}
                        className={"blob" + (s.starred ? " star" : "")}
                        style={{
                          left: `calc(${left}% + 6px)`,
                          width: `calc(${right - left}% - 6px)`,
                          borderLeftColor: stage.color,
                        }}>
                        {s.starred && "★ "}{s.artist.split(" ")[0]}
                      </div>
                    );
                  })}
                </div>
              );
            })}
            {/* Hatched warning zones */}
            {clashZones.map((z, i) => (
              <div key={i} className="clashmark" style={{
                left: `calc(${xPct(z.start)}% + 6px)`,
                width: `calc(${xPct(z.end) - xPct(z.start)}% )`,
              }}></div>
            ))}
          </div>
          <div style={{
            marginTop: 10,
            display: "flex", gap: 12,
            fontFamily: "var(--font-mono)", fontSize: 9,
            textTransform: "uppercase", letterSpacing: "0.08em",
          }}>
            <span style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--fg-3)" }}>
              <span style={{ width: 14, height: 6, background: "var(--surface-2)", borderLeft: "2px solid var(--fg-3)" }}></span>
              SCHEDULED
            </span>
            <span style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--fg-2)" }}>
              <span style={{ width: 14, height: 6, background: "var(--accent-wash)", borderLeft: "2px solid var(--accent)" }}></span>
              ★ STARRED
            </span>
            <span style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--warn)" }}>
              <span style={{ width: 14, height: 6, background: "repeating-linear-gradient(-45deg, rgba(255,179,71,0.4) 0 3px, transparent 3px 6px)" }}></span>
              CLASH
            </span>
          </div>
        </div>

        <div className="clash-list-head">
          <span className="l">! RESOLVE</span>
          <span className="r">{clashPairs.length} conflict{clashPairs.length === 1 ? "" : "s"}</span>
        </div>

        {clashPairs.map(([a, b], i) => (
          <ClashCard key={i} a={a} b={b} stageA={stageById[a.stage]} stageB={stageById[b.stage]} />
        ))}

        <div style={{ height: 80 }}></div>
      </div>

      <Caption
        name="V5 // CLASH RADAR"
        desc="Visual diff of your starred sets. Stage lanes show every artist with hatched magenta zones where your stars overlap, plus a per-clash resolver below."
      />
    </div>
  );
}

function ClashCard({ a, b, stageA, stageB }) {
  const [chosen, setChosen] = useState("a");
  return (
    <div className="clash-card">
      <div className="when">
        OVERLAP {fmtTime(Math.max(a.t, b.t))} → {fmtTime(Math.min(a.t + a.dur, b.t + b.dur))} ·{" "}
        {Math.min(a.t + a.dur, b.t + b.dur) - Math.max(a.t, b.t)} MIN
      </div>
      <div className="pair">
        <button className={"opt" + (chosen === "a" ? " chosen" : "")} style={{ borderLeftColor: stageA.color }}
          onClick={() => setChosen("a")}>
          <span className="a">★ {a.artist}</span>
          <span className="b">{stageA.name} · {fmtTime(a.t)} → {fmtTime(a.t + a.dur)}</span>
        </button>
        <span className="vs">VS</span>
        <button className={"opt" + (chosen === "b" ? " chosen" : "")} style={{ borderLeftColor: stageB.color }}
          onClick={() => setChosen("b")}>
          <span className="a">★ {b.artist}</span>
          <span className="b">{stageB.name} · {fmtTime(b.t)} → {fmtTime(b.t + b.dur)}</span>
        </button>
      </div>
      <div className="actions">
        <button className="chip">SPLIT — 30M EACH</button>
        <button className="chip">UNSTAR {chosen === "a" ? b.artist.split(" ")[0] : a.artist.split(" ")[0]}</button>
      </div>
    </div>
  );
}

/* ──────────────────────────────────────────────────────────────
   V6 — NOW-STRIP (Departures Board)
   Top: big countdown + currently playing hero.
   Bottom: a Solari/transit-style departures board listing every
   imminent set sorted by start time with status flags.
   ────────────────────────────────────────────────────────────── */
function V6NowStrip() {
  const now = NOW.t;
  const stageById = useMemo(() => {
    const m = {}; STAGES.forEach(s => m[s.id] = s); return m;
  }, []);
  const sets = SETS.filter(s => s.day === NOW.day);
  const live = sets.find(s => s.live);
  const liveStage = live ? stageById[live.stage] : null;

  // Next 4 hours
  const upcoming = sets
    .filter(s => s.t > now && s.t < now + 240)
    .sort((a, b) => a.t - b.t)
    .slice(0, 8);

  // First starred next
  const nextStarred = sets.filter(s => s.starred && s.t > now).sort((a, b) => a.t - b.t)[0];
  const diff = nextStarred ? nextStarred.t - now : 0;
  const hh = String(Math.floor(diff / 60)).padStart(2, "0");
  const mm = String(diff % 60).padStart(2, "0");
  const ss = "28";

  return (
    <div className="phone" data-screen-label="V6 Now strip">
      <StatusBar />
      <TopNav festival="Field Day" right={<NavRightStandard />} />

      <div className="main">
        {/* Hero — currently playing */}
        <div className="ns-hero">
          <div className="lbl">
            <span className="live-dot-mini"></span>
            // ON NOW · {liveStage.name}
          </div>
          <h1>{live.artist}</h1>
          <div className="where">{fmtTime(live.t)} → {fmtTime(live.t + live.dur)} · {live.genre}</div>

          {nextStarred && (
            <div className="countdown">
              <span className="big">T−{hh}:{mm}:{ss}</span>
              <span className="lbl2">
                next ★ {nextStarred.artist}
              </span>
            </div>
          )}
        </div>

        {/* Departures board */}
        <div className="dep-head">
          <div>TIME</div>
          <div></div>
          <div>ARTIST · STAGE</div>
          <div>STATUS</div>
        </div>

        {upcoming.map(s => {
          const stage = stageById[s.stage];
          const inMin = s.t - now;
          let status, statusClass;
          if (s.starred)         { status = "★ STARRED"; statusClass = "acc"; }
          else if (s.clashes)    { status = "! CLASH";   statusClass = "warn"; }
          else if (inMin <= 15)  { status = `T−${inMin}M`; statusClass = "acc"; }
          else                   { status = "QUEUED";   statusClass = "dim"; }
          return (
            <button key={s.id} className={"dep-row" + (s.starred ? " starred" : "")} onClick={() => {}}>
              <div className="t">{fmtTime(s.t)}</div>
              <div className="bar" style={{ background: stage.color }}></div>
              <div style={{ minWidth: 0 }}>
                <div className="nm">{s.artist}</div>
                <div className="sub">{stage.name} · {s.dur}M · {s.genre}</div>
              </div>
              <div className={"status " + statusClass}>{status}</div>
            </button>
          );
        })}

        <div style={{ height: 90 }}></div>
      </div>

      <Caption
        name="V6 // NOW-STRIP"
        desc="Departures-board view. What's playing now, countdown to your next star, and every imminent set listed train-station style with status flags."
      />
    </div>
  );
}

Object.assign(window, { V4Filters, V5ClashRadar, V6NowStrip });
