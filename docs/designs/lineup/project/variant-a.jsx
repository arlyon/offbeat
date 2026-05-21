/* Variant A — "Index"
   Conservative, by-the-book. TopNav + bottom tabs. Dotted search.
   Festivals list with pinned-saved section above discover.
   ────────────────────────────────────────────────────────── */

function VariantA() {
  const [q, setQ] = useState("");
  const [saved, setSaved] = useState({ fieldday26: true, ade25: true });
  const [tab, setTab] = useState("home");

  const fests = window.FESTS;
  const filtered = useMemo(() =>
    fests.filter(f =>
      !q ||
      f.name.toLowerCase().includes(q.toLowerCase()) ||
      f.city.toLowerCase().includes(q.toLowerCase()) ||
      f.genres.some(g => g.toLowerCase().includes(q.toLowerCase()))
    ), [fests, q]);

  const savedFests   = filtered.filter(f => saved[f.id]);
  const discoverFests = filtered.filter(f => !saved[f.id]);

  return (
    <div className="phone va">
      <StatusBar time="20:30" />

      {/* Top nav */}
      <div className="va-topnav">
        <div className="va-wm">
          <Mark />
          <span className="wordmark">OFFBEAT<span className="slash">//</span></span>
        </div>
        <div className="va-actions">
          <button className="va-iconbtn"><Icon name="WifiOff" size={16} /></button>
          <button className="va-iconbtn"><Icon name="Settings2" size={17} /></button>
        </div>
      </div>

      <div className="main">
        {/* Page header */}
        <div className="va-head">
          <div className="va-title">Festivals.</div>
          <div className="va-sub">
            <span>{fests.filter(f => f.status !== "past").length} ACTIVE</span>
            <span className="dot">·</span>
            <span>{Object.values(saved).filter(Boolean).length} SAVED</span>
            <span className="dot">·</span>
            <span>SYNC 14:02</span>
          </div>
        </div>

        {/* Search */}
        <div className="va-searchwrap">
          <label className="va-search">
            <Icon name="Search" size={16} color="var(--fg-3)" />
            <input
              type="text"
              value={q}
              onChange={e => setQ(e.target.value)}
              placeholder="search festivals, cities, genres"
              spellCheck={false}
            />
            {q && (
              <button className="va-clear" onClick={() => setQ("")}>
                <Icon name="X" size={14} />
              </button>
            )}
            <span className="va-kbd">⌘K</span>
          </label>
        </div>

        {/* Saved section */}
        {savedFests.length > 0 && (
          <>
            <div className="va-eyebrow">
              <span className="va-eyebrow-l">
                <span>// SAVED</span>
                <span className="va-eyebrow-pill">★ {savedFests.length}</span>
              </span>
              <span className="va-eyebrow-r">EDIT</span>
            </div>
            {savedFests.map(f => (
              <FestRowA
                key={f.id}
                fest={f}
                saved={!!saved[f.id]}
                onToggle={() => setSaved(s => ({ ...s, [f.id]: !s[f.id] }))}
              />
            ))}
          </>
        )}

        {/* Discover */}
        <div className="va-eyebrow">
          <span className="va-eyebrow-l"><span>// DISCOVER</span></span>
          <span className="va-eyebrow-r">{discoverFests.length} FESTIVALS</span>
        </div>
        {discoverFests.map(f => (
          <FestRowA
            key={f.id}
            fest={f}
            saved={!!saved[f.id]}
            onToggle={() => setSaved(s => ({ ...s, [f.id]: !s[f.id] }))}
          />
        ))}

        {filtered.length === 0 && (
          <div className="va-empty">
            NO RESULTS // <span style={{ color: "var(--accent)" }}>"{q}"</span>
          </div>
        )}

        <div style={{ height: 24 }}></div>
      </div>

      {/* Bottom tab bar */}
      <nav className="va-tabbar">
        <TabA id="home"     label="FESTIVALS" icon="Music"         active={tab} onClick={setTab} />
        <TabA id="schedule" label="SCHEDULE"  icon="CalendarClock" active={tab} onClick={setTab} />
        <TabA id="now"      label="NOW"       icon="Radio"         active={tab} onClick={setTab} liveDot />
        <TabA id="you"      label="YOU"       icon="Star"          active={tab} onClick={setTab} />
      </nav>
    </div>
  );
}

function TabA({ id, label, icon, active, onClick, liveDot }) {
  const isActive = active === id;
  return (
    <button className={"va-tab" + (isActive ? " active" : "")} onClick={() => onClick(id)}>
      <span className="va-tab-icon">
        <Icon name={icon} size={18} />
        {liveDot && <span className="va-tab-livedot"><span className="live-mini"></span></span>}
      </span>
      <span>{label}</span>
    </button>
  );
}

function FestRowA({ fest, saved, onToggle }) {
  return (
    <div className="va-row">
      <FestArt hue={fest.hue} w={68} h={68} label={fest.id.slice(0, 3)} />
      <div className="va-row-body">
        <div className="va-row-head">
          <div className="va-row-name">
            {fest.name}
            {fest.status === "live" && (
              <span className="va-livebadge"><span className="live-mini"></span> LIVE</span>
            )}
            {fest.status === "past" && <span className="va-pastbadge">2025</span>}
          </div>
          <button
            className={"star-btn" + (saved ? " on" : "")}
            onClick={(e) => { e.stopPropagation(); onToggle(); }}
          >
            {saved ? "★" : "☆"}
          </button>
        </div>
        <div className="va-row-meta">
          <span>{fest.dates.replace("· 2025", "").trim()}</span>
          <span>·</span>
          <span>{fest.city}</span>
        </div>
        <div className="va-row-meta dim">
          <span>{fest.stages} STAGES</span>
          <span>·</span>
          <span>{fest.sets || "—"} SETS</span>
          <span>·</span>
          <span>{fest.genres[0]}</span>
        </div>
      </div>
    </div>
  );
}

window.VariantA = VariantA;
