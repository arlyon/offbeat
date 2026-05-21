/* Festival Views — variants
   ──────────────────────────────────────────────────────────── */

/* ──────────────────────────────────────────────────────────────
   V1 — GANTT-SCROLL
   Time runs on X axis, stages on Y axis. The user scrolls
   vertically on the page; that scroll translates the gantt
   horizontally. Feels mobile-native, looks like a timetable.
   ────────────────────────────────────────────────────────────── */
function V1GanttScroll() {
  // Time window
  const START   = 18 * 60;       // 18:00
  const END     = 26 * 60;       // 02:00 next day
  const RANGE   = END - START;   // 480 min
  const PX_PER_MIN = 3;          // 480 * 3 = 1440 px wide content
  const STAGE_LABEL_W = 46;
  const CONTENT_W = RANGE * PX_PER_MIN;

  const [day, setDay] = useState("fri");
  const sets = SETS.filter(s => s.day === day);

  const hostRef = useRef(null);
  const ganttRef = useRef(null);
  const [progress, setProgress] = useState(0);   // 0..1
  const [viewportInnerW, setViewportInnerW] = useState(0);

  // Measure viewport inner width (artboard width minus stage label gutter)
  useLayoutEffect(() => {
    if (!ganttRef.current) return;
    const ro = new ResizeObserver(() => {
      const w = ganttRef.current.clientWidth - STAGE_LABEL_W;
      setViewportInnerW(w);
    });
    ro.observe(ganttRef.current);
    return () => ro.disconnect();
  }, []);

  const maxTx = Math.max(0, CONTENT_W - viewportInnerW);

  // Scroll → progress
  const onScroll = useCallback(() => {
    const el = hostRef.current;
    if (!el) return;
    const max = el.scrollHeight - el.clientHeight;
    if (max <= 0) { setProgress(0); return; }
    const p = Math.max(0, Math.min(1, el.scrollTop / max));
    setProgress(p);
  }, []);

  // Auto-center on "now" once at mount
  useEffect(() => {
    const el = hostRef.current;
    if (!el || !viewportInnerW) return;
    const nowX = (NOW.t - START) * PX_PER_MIN;
    const targetTx = Math.max(0, Math.min(maxTx, nowX - viewportInnerW / 2));
    const targetP = maxTx > 0 ? targetTx / maxTx : 0;
    const targetScroll = targetP * (el.scrollHeight - el.clientHeight);
    el.scrollTop = targetScroll;
    setProgress(targetP);
  }, [viewportInnerW, day]); // eslint-disable-line

  const tx = progress * maxTx;

  // What time is "centered" right now?
  const centerMin = START + (tx + viewportInnerW / 2) / PX_PER_MIN;
  const centerH = Math.floor(centerMin / 60) % 24;
  const centerM = Math.floor(centerMin % 60);
  const centerStr =
    String(centerH).padStart(2, "0") + ":" + String(centerM).padStart(2, "0");

  // Now line X (relative to inner content, before translate)
  const nowX = (NOW.t - START) * PX_PER_MIN;

  // Hours on the axis
  const axisHours = useMemo(() => {
    const arr = [];
    for (let m = START; m <= END; m += 60) arr.push(m);
    return arr;
  }, []);

  return (
    <div className="phone" data-screen-label="V1 Gantt scroll">
      <StatusBar time="20:30" />
      <TopNav
        festival="Field Day"
        right={<NavRightStandard />}
      />

      {/* Day picker — compact pills */}
      <div className="gs-meta-strip">
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <div className="lbl">// NOW <span className="live-dot-mini" style={{ marginLeft: 6 }}></span></div>
          <div style={{ fontFamily: "var(--font-mono)", fontSize: 12, color: "var(--fg)", fontVariantNumeric: "tabular-nums" }}>
            20:30
          </div>
        </div>
        <div className="day-pill" style={{ marginLeft: "auto", padding: 0, border: 0, background: "transparent" }}>
          {DAYS.map(d => (
            <button key={d.id}
              onClick={() => setDay(d.id)}
              style={{
                padding: "6px 10px",
                background: d.id === day ? "var(--fg)" : "transparent",
                color: d.id === day ? "var(--bg)" : "var(--fg-2)",
                border: "1.5px dotted " + (d.id === day ? "var(--fg)" : "var(--dotted)"),
                borderStyle: d.id === day ? "solid" : "dotted",
                marginLeft: 4,
                fontFamily: "var(--font-mono)", fontSize: 10, fontWeight: 700,
                textTransform: "uppercase", letterSpacing: "0.08em",
                cursor: "pointer",
              }}>
              {d.label} <span style={{ marginLeft: 4, opacity: 0.7 }}>{d.num}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Gantt viewport */}
      <div className="gs-viewport">
        {/* The scroll host is the user's input — they scroll this vertically. */}
        <div className="gs-scroll-host" ref={hostRef} onScroll={onScroll}>
          {/* Sentinel — height controls how much "scroll travel" exists.
              ~1500px gives a comfortable, fast pan across 8h of festival. */}
          <div className="gs-scroll-sentinel" style={{ height: 1500 + "px" }}></div>
        </div>

        {/* Gantt itself, absolutely positioned over the host. Pointer events
            are off on the host overlay so the user can click set blocks. */}
        <div className="gs-gantt" ref={ganttRef} style={{ pointerEvents: "none" }}>
          {/* Time axis (top) */}
          <div className="gs-axis">
            <div className="gs-axis-inner" style={{ transform: `translateX(${STAGE_LABEL_W - tx}px)` }}>
              {axisHours.map((m, i) => (
                <div key={i} className="gs-axis-tick" style={{ width: 60 * PX_PER_MIN }}>
                  {fmtTime(m)}
                  <div className="half" style={{ left: 30 * PX_PER_MIN }}></div>
                </div>
              ))}
            </div>
            {/* Sticky "centered time" badge */}
            <div style={{
              position: "absolute", right: 8, top: 8,
              fontFamily: "var(--font-mono)", fontSize: 11, fontWeight: 700,
              color: "var(--accent)", letterSpacing: "-0.02em",
              fontVariantNumeric: "tabular-nums",
              background: "var(--bg)",
              padding: "2px 6px",
              border: "1.5px dotted var(--accent)",
            }}>
              {centerStr}
            </div>
          </div>

          {/* Stage rows */}
          <div className="gs-stages">
            <div className="gs-stages-inner" style={{
              width: CONTENT_W + STAGE_LABEL_W,
              transform: `translateX(${-tx}px)`,
              left: 0,
              right: "auto",
            }}>
              {STAGES.map((stage, i) => (
                <div key={stage.id} className="gs-stage-row">
                  {sets
                    .filter(s => s.stage === stage.id)
                    .map(s => {
                      const left = (s.t - START) * PX_PER_MIN + STAGE_LABEL_W;
                      const width = s.dur * PX_PER_MIN;
                      return (
                        <div
                          key={s.id}
                          className={
                            "gs-set" +
                            (s.starred ? " starred" : "") +
                            (s.live ? " live" : "")
                          }
                          style={{
                            left, width,
                            borderLeftColor: stage.color,
                            pointerEvents: "auto",
                          }}
                        >
                          <div className="n">
                            {s.starred && <span className="star">★</span>}
                            {s.live && <span className="live-dot-mini"></span>}
                            <span>{s.artist}</span>
                          </div>
                          <div className="t">{fmtTime(s.t)} → {fmtTime(s.t + s.dur)}</div>
                        </div>
                      );
                    })}
                </div>
              ))}
            </div>

            {/* Sticky stage labels on the LEFT (they don't translate) */}
            {STAGES.map((stage, i) => (
              <div
                key={stage.id}
                className="gs-stage-label"
                style={{
                  top: `calc(${(i / STAGES.length) * 100}% + 1px)`,
                  height: `calc(${100 / STAGES.length}% - 1.5px)`,
                }}
              >
                <span className="lc">{stage.short}</span>
                <div className="sw" style={{ background: stage.color }}></div>
              </div>
            ))}

            {/* Now line — only drawn if "now" falls in current view */}
            {NOW.day === day && (
              <div
                className="gs-now-line"
                style={{ left: STAGE_LABEL_W + nowX - tx }}
              />
            )}
          </div>
        </div>

        {/* HUD — bottom scrubber + scroll hint */}
        <div className="gs-hud">
          <div className="scrubber">
            <div className="fill" style={{ width: `${progress * 100}%` }}></div>
            <div className="head" style={{ left: `calc(${progress * 100}% - 1.5px)` }}></div>
            <div className="lbl">
              {fmtTime(START + tx / PX_PER_MIN)} → {fmtTime(START + (tx + viewportInnerW) / PX_PER_MIN)}
            </div>
          </div>
          <div className="hint">
            scroll
            <span className="arrow">↓</span>
          </div>
        </div>
      </div>

      <Caption
        name="V1 // GANTT-SCROLL"
        desc="Time on X, stages on Y. Page-scroll vertically pans the timeline horizontally — feels mobile-native, reads like a backstage timetable. Time pill on the right shows the centered minute."
      />
    </div>
  );
}

/* ──────────────────────────────────────────────────────────────
   V2 — DAY-TABS HERO + HOUR-GROUPED LIST
   Big ticket-stub day picker. Below, a list of sets grouped by
   hour, with sticky hour headers as you scroll.
   ────────────────────────────────────────────────────────────── */
function V2DayTabs() {
  const [day, setDay] = useState("fri");
  const sets = SETS.filter(s => s.day === day).sort((a, b) => a.t - b.t);
  const stageById = useMemo(() => {
    const m = {}; STAGES.forEach(s => m[s.id] = s); return m;
  }, []);

  // Group by hour bucket
  const grouped = useMemo(() => {
    const g = new Map();
    sets.forEach(s => {
      const hr = Math.floor(s.t / 60);
      if (!g.has(hr)) g.set(hr, []);
      g.get(hr).push(s);
    });
    return [...g.entries()].sort((a, b) => a[0] - b[0]);
  }, [sets]);

  return (
    <div className="phone" data-screen-label="V2 Day tabs">
      <StatusBar />
      <TopNav festival="Field Day 2026" right={<NavRightStandard filterCount={2} />} />

      <div className="page-title">Set times.</div>
      <div className="page-sub">{FESTIVAL.where} <span style={{ color: "var(--fg-4)" }}>|</span> {sets.length} sets</div>

      {/* Day strip — ticket-stub feel */}
      <div className="daytab-strip" style={{ gridTemplateColumns: `repeat(${DAYS.length}, 1fr)` }}>
        {DAYS.map(d => {
          const ct = SETS.filter(s => s.day === d.id).length;
          return (
            <button key={d.id} className={"daytab" + (d.id === day ? " active" : "")} onClick={() => setDay(d.id)}>
              <span className="mo">// {d.month}</span>
              <span className="dow">{d.label}</span>
              <span className="num">{d.num}</span>
              <span className="ct">{ct} sets</span>
            </button>
          );
        })}
      </div>

      <div className="main">
        {grouped.map(([hr, arr]) => (
          <React.Fragment key={hr}>
            <div className="hr-row">
              <span className="hr">{String(hr % 24).padStart(2, "0")}:00</span>
              <span className="lbl">→ {String((hr + 1) % 24).padStart(2, "0")}:00</span>
              <span className="ct">{arr.length} sets</span>
            </div>
            {arr.map(s => (
              <SetRow key={s.id} set={s} stage={stageById[s.stage]} onOpen={() => {}} onStar={() => {}} />
            ))}
          </React.Fragment>
        ))}
        <div style={{ height: 80 }}></div>
      </div>

      <Caption
        name="V2 // DAY TABS"
        desc="Multi-day festivals: ticket-stub day picker up top, sets grouped by hour. Sticky hour headers keep your place as you scroll a dense list."
      />
    </div>
  );
}

/* ──────────────────────────────────────────────────────────────
   V3 — STAGE TABS
   Top-scrolling stage tabs with color swatches + live flag.
   Selected stage gets a hero card and a vertical lineup.
   ────────────────────────────────────────────────────────────── */
function V3StageTabs() {
  const [stageId, setStageId] = useState("s1");
  const [day, setDay] = useState("fri");
  const stage = STAGES.find(s => s.id === stageId);
  const sets = SETS.filter(s => s.day === day && s.stage === stageId).sort((a, b) => a.t - b.t);
  const live = sets.find(s => s.live);
  const setsByStage = useMemo(() => {
    const m = {}; STAGES.forEach(s => m[s.id] = []);
    SETS.filter(s => s.day === day).forEach(s => m[s.stage].push(s));
    return m;
  }, [day]);

  return (
    <div className="phone" data-screen-label="V3 Stage tabs">
      <StatusBar />
      <TopNav festival="Field Day 2026" right={<NavRightStandard />} />

      {/* Day pill row */}
      <div style={{
        display: "flex", gap: 8, padding: "10px 14px",
        borderBottom: "1.5px dotted var(--dotted)",
      }}>
        <span style={{
          fontFamily: "var(--font-mono)", fontSize: 9, fontWeight: 700,
          color: "var(--fg-3)", letterSpacing: "0.1em",
          textTransform: "uppercase", alignSelf: "center",
        }}>// DAY</span>
        {DAYS.map(d => (
          <button key={d.id}
            className={"chip" + (d.id === day ? " active" : "")}
            onClick={() => setDay(d.id)}
          >{d.label} {d.num}</button>
        ))}
      </div>

      {/* Stage tabs — horizontally scrolling */}
      <div className="stage-tabs">
        {STAGES.map(s => {
          const liveOn = SETS.some(x => x.stage === s.id && x.day === day && x.live);
          return (
            <button
              key={s.id}
              className={"stage-tab" + (s.id === stageId ? " active" : "")}
              onClick={() => setStageId(s.id)}
              style={{ color: s.color }}
            >
              <div className="sw" style={{ background: s.color }}></div>
              <div className="nm">{s.name}</div>
              <div className="ct">{setsByStage[s.id].length} sets</div>
              {liveOn && <span className="live-flag"><span className="live-dot-mini"></span></span>}
            </button>
          );
        })}
      </div>

      <div className="main">
        {/* Stage hero */}
        <div className="stage-hero">
          <div className="accent-stripe" style={{ background: stage.color }}></div>
          <div className="super">// STAGE PROFILE</div>
          <h1 className="h1">{stage.name}</h1>
          <div className="meta">
            <span>{sets.length} SETS</span>
            <span style={{ color: "var(--fg-4)" }}>|</span>
            <span>{sets.reduce((a, s) => a + s.dur, 0) / 60}H PROGRAMMING</span>
            <span style={{ color: "var(--fg-4)" }}>|</span>
            <span>{fmtTime(sets[0]?.t || 0)} → {fmtTime((sets[sets.length-1]?.t || 0) + (sets[sets.length-1]?.dur || 0))}</span>
          </div>
        </div>

        {/* Now-on-this-stage callout (only if live) */}
        {live && (
          <div className="now-callout">
            <span className="live-dot-mini ld"></span>
            <div className="col">
              <span className="nm">{live.artist}</span>
              <span className="sub">{fmtTime(live.t)} → {fmtTime(live.t + live.dur)} · {live.genre}</span>
            </div>
            <span className="badge">LIVE</span>
          </div>
        )}

        <div className="eyebrow" style={{ paddingTop: 20 }}>
          <span className="label">// LINEUP</span>
          <span className="meta">{DAYS.find(d => d.id === day).label} {DAYS.find(d => d.id === day).num}</span>
        </div>

        {sets.map(s => (
          <button key={s.id} className="bigcard" onClick={() => {}}>
            <div className="t">
              {fmtTime(s.t)}
              <span className="e">→ {fmtTime(s.t + s.dur)}</span>
            </div>
            <div style={{ minWidth: 0 }}>
              <div className="nm">{s.artist}</div>
              <div className="meta">
                {s.live && <span className="live-dot-mini"></span>}
                <span>{s.dur} MIN</span>
                <span style={{ color: "var(--fg-4)" }}>|</span>
                <span>{s.genre}</span>
                {s.clashes && s.clashes.length > 0 && <>
                  <span style={{ color: "var(--fg-4)" }}>|</span>
                  <span style={{ color: "var(--warn)" }}>! CLASH</span>
                </>}
              </div>
            </div>
            <span className={"star" + (s.starred ? " on" : "")}>{s.starred ? "★" : "☆"}</span>
          </button>
        ))}

        <div style={{ height: 80 }}></div>
      </div>

      <Caption
        name="V3 // STAGE TABS"
        desc="Plan your night stage-by-stage. Horizontal-scrolling tabs with color swatches and live flags. Selected stage gets a profile header and full lineup."
      />
    </div>
  );
}

Object.assign(window, { V1GanttScroll, V2DayTabs, V3StageTabs });
