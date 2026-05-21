/* Variant B — "Stub Stack"
   Motif-heavy. Tear-off ticket stubs as festival cards.
   Inline search in the chrome. Segmented filter. Floating dock.
   ────────────────────────────────────────────────────────── */

function VariantB() {
  const [q, setQ] = useState("");
  const [seg, setSeg] = useState("all");                 // saved | all | nearby | past
  const [saved, setSaved] = useState({ fieldday26: true, ade25: true });
  const [tab, setTab] = useState("home");
  const fests = window.FESTS;

  const visible = useMemo(() => {
    let list = fests;
    if (seg === "saved")  list = list.filter(f => saved[f.id]);
    if (seg === "nearby") list = list.filter(f => f.cc === "UK");
    if (seg === "past")   list = list.filter(f => f.status === "past");
    if (seg === "all")    list = list.filter(f => f.status !== "past");
    return list.filter(f =>
      !q ||
      f.name.toLowerCase().includes(q.toLowerCase()) ||
      f.city.toLowerCase().includes(q.toLowerCase())
    );
  }, [fests, q, seg, saved]);

  const counts = {
    saved:  fests.filter(f => saved[f.id]).length,
    all:    fests.filter(f => f.status !== "past").length,
    nearby: fests.filter(f => f.cc === "UK").length,
    past:   fests.filter(f => f.status === "past").length,
  };

  return (
    <div className="phone vb">
      <StatusBar time="20:30" />

      {/* Top chrome */}
      <div className="vb-topnav">
        <div className="vb-wm-row">
          <span className="wordmark vb-wm">
            <Mark />
            <span style={{ marginLeft: 8 }}>OFFBEAT<span className="slash">//</span></span>
          </span>
          <button className="vb-iconbtn"><Icon name="MapPin" size={16} /></button>
        </div>
        <div className="vb-searchbox">
          <Icon name="Search" size={15} color="var(--fg-3)" />
          <input
            type="text"
            value={q}
            onChange={e => setQ(e.target.value)}
            placeholder="festivals, cities, headliners"
            spellCheck={false}
          />
          {q && (
            <button className="vb-clear" onClick={() => setQ("")}>
              <Icon name="X" size={12} />
            </button>
          )}
        </div>
      </div>

      {/* Segmented filter */}
      <div className="vb-segmented">
        <SegB id="saved"  label="SAVED"  count={counts.saved}  current={seg} onClick={setSeg} />
        <SegB id="all"    label="ALL"    count={counts.all}    current={seg} onClick={setSeg} />
        <SegB id="nearby" label="NEARBY" count={counts.nearby} current={seg} onClick={setSeg} />
        <SegB id="past"   label="PAST"   count={counts.past}   current={seg} onClick={setSeg} />
      </div>

      {/* Stub stack */}
      <div className="main vb-main">
        {visible.length === 0 && (
          <div className="vb-empty">
            <div>NO STUBS</div>
            <div className="vb-empty-sub">try another segment</div>
          </div>
        )}
        {visible.map((f, i) => (
          <StubB
            key={f.id}
            fest={f}
            saved={!!saved[f.id]}
            onToggle={() => setSaved(s => ({ ...s, [f.id]: !s[f.id] }))}
            seq={i + 1}
          />
        ))}
        <div style={{ height: 80 }}></div>
      </div>

      {/* Floating dock */}
      <div className="vb-dock-wrap">
        <nav className="vb-dock">
          <DockB id="home"     icon="Music"         label="FEST"   active={tab} onClick={setTab} />
          <DockB id="schedule" icon="CalendarClock" label="SCHED"  active={tab} onClick={setTab} />
          <DockB id="now"      icon="Radio"         label="NOW"    active={tab} onClick={setTab} liveDot />
          <DockB id="you"      icon="Star"          label="YOU"    active={tab} onClick={setTab} />
        </nav>
      </div>
    </div>
  );
}

function SegB({ id, label, count, current, onClick }) {
  const isActive = current === id;
  return (
    <button
      className={"vb-seg" + (isActive ? " active" : "")}
      onClick={() => onClick(id)}
    >
      <span>{label}</span>
      <span className="vb-seg-count">{count}</span>
    </button>
  );
}

function StubB({ fest, saved, onToggle, seq }) {
  const [m1, d1] = fest.dateRange[0].split(" ");
  const [, d2]   = fest.dateRange[1].split(" ");
  const countdown = window.fmtCountdown(fest.daysAway);
  const isPast = fest.status === "past";
  const isLive = fest.status === "live";

  return (
    <div className={"vb-stub" + (isPast ? " past" : "")}>
      <div className="tear-top vb-tearedge"></div>

      {/* meta row */}
      <div className="vb-stub-meta">
        <span className="vb-stub-seq">№ {String(seq).padStart(2, "0")} / {fest.year}</span>
        <span className="vb-stub-cd" data-live={isLive}>
          {isLive ? <><span className="live-mini"></span> LIVE NOW</> : countdown}
        </span>
      </div>

      <div className="dot-rule" style={{ margin: "0 16px" }}></div>

      {/* hero strip */}
      <div className="vb-stub-hero">
        <FestArt hue={fest.hue} w="100%" h={156} style={{ width: "100%" }} />
        <div className="vb-stub-hero-overlay">
          <div className="vb-stub-headliners">
            {fest.headliners.slice(0, 3).map((h, i) => (
              <span key={h}>
                {i > 0 && <span className="sep">·</span>}
                {h}
              </span>
            ))}
          </div>
        </div>
        <button
          className={"vb-stub-star star-btn" + (saved ? " on" : "")}
          onClick={(e) => { e.stopPropagation(); onToggle(); }}
        >
          {saved ? "★" : "☆"}
        </button>
      </div>

      {/* identity */}
      <div className="vb-stub-body">
        <div className="vb-stub-title-row">
          <div className="vb-stub-dates">
            <span className="vb-stub-month">{m1}</span>
            <span className="vb-stub-day">{d1}</span>
            <span className="vb-stub-arrow">→</span>
            <span className="vb-stub-day">{d2}</span>
          </div>
          <div className="vb-stub-name-wrap">
            <div className="vb-stub-name">{fest.name}</div>
            <div className="vb-stub-loc">
              {fest.city} <span className="muted">// {fest.cc}</span>
            </div>
          </div>
        </div>

        <div className="vb-stub-tags">
          <span className="vb-tag">{fest.stages} STAGES</span>
          <span className="vb-tag">{fest.sets || "—"} SETS</span>
          {fest.genres.map(g => <span key={g} className="vb-tag dim">{g}</span>)}
        </div>
      </div>

      <div className="tear-bottom vb-tearedge"></div>
    </div>
  );
}

function DockB({ id, icon, label, active, onClick, liveDot }) {
  const isActive = active === id;
  return (
    <button
      className={"vb-dock-btn" + (isActive ? " active" : "")}
      onClick={() => onClick(id)}
    >
      <span style={{ position: "relative", display: "inline-flex" }}>
        <Icon name={icon} size={17} />
        {liveDot && (
          <span style={{ position: "absolute", top: -3, right: -4 }}>
            <span className="live-mini"></span>
          </span>
        )}
      </span>
      <span className="vb-dock-label">{label}</span>
    </button>
  );
}

window.VariantB = VariantB;
