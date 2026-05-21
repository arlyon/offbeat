/* Variant C — "Console"
   Command-line brutalist. Mono table of festivals.
   Top: prompt-style search. Bottom: keyboard-cue status bar instead of tabs.
   ────────────────────────────────────────────────────────── */

function VariantC() {
  const [q, setQ] = useState("");
  const [scope, setScope] = useState("/all");      // /all /saved /nearby /past
  const [saved, setSaved] = useState({ fieldday26: true, ade25: true, primavera26: true });
  const [selected, setSelected] = useState(0);
  const [tab, setTab] = useState("F1");
  const fests = window.FESTS;

  const visible = useMemo(() => {
    let list = fests;
    if (scope === "/saved")  list = list.filter(f => saved[f.id]);
    if (scope === "/nearby") list = list.filter(f => f.cc === "UK");
    if (scope === "/past")   list = list.filter(f => f.status === "past");
    if (scope === "/all")    list = list.filter(f => f.status !== "past");
    return list.filter(f =>
      !q ||
      f.name.toLowerCase().includes(q.toLowerCase()) ||
      f.city.toLowerCase().includes(q.toLowerCase()) ||
      f.genres.some(g => g.toLowerCase().includes(q.toLowerCase()))
    );
  }, [fests, q, scope, saved]);

  const matchSuffix = scope === "/all" ? "" : ` ${scope}`;

  return (
    <div className="phone vc">
      <StatusBar time="20:30" />

      {/* Brand strip — minimal */}
      <div className="vc-brandstrip">
        <span className="vc-brand">
          <Mark />
          <span>OFFBEAT<span className="vc-slash">//</span><span className="vc-mod">FESTS</span></span>
        </span>
        <span className="vc-conn">
          <span className="live-mini"></span>
          <span>OFFLINE · CACHED 14:02</span>
        </span>
      </div>

      {/* Command bar (search) */}
      <div className="vc-prompt-row">
        <span className="vc-prompt">
          <span className="vc-prompt-host">OFFBEAT</span>
          <span className="vc-prompt-sep">:</span>
          <span className="vc-prompt-scope">{scope.replace("/", "")}</span>
          <span className="vc-prompt-arrow">&gt;</span>
        </span>
        <input
          className="vc-input"
          type="text"
          value={q}
          onChange={e => setQ(e.target.value)}
          placeholder="search // / for scope"
          spellCheck={false}
        />
        <span className="vc-caret">█</span>
      </div>

      {/* Scope tabs (slash-commands) */}
      <div className="vc-scopes">
        <ScopeC id="/all"    label="all"    current={scope} onClick={setScope} />
        <ScopeC id="/saved"  label="saved"  current={scope} onClick={setScope}
          badge={fests.filter(f => saved[f.id]).length} />
        <ScopeC id="/nearby" label="nearby" current={scope} onClick={setScope} />
        <ScopeC id="/past"   label="past"   current={scope} onClick={setScope} />
        <span className="vc-scope-meta">
          MATCH {visible.length}/{fests.length}{matchSuffix}
        </span>
      </div>

      {/* Live ticker */}
      <div className="vc-ticker">
        <span className="vc-ticker-badge"><span className="live-mini"></span> LIVE</span>
        <span className="vc-ticker-text">
          FIELD DAY · FOUR TET // STAGE 1 · CARIBOU // STAGE 1 · BICEP // STAGE 2 · HELENA HAUFF // RED ROOM
        </span>
      </div>

      {/* Column headers */}
      <div className="vc-thead">
        <span className="vc-c0"></span>
        <span className="vc-c1">NAME</span>
        <span className="vc-c2">DATES</span>
        <span className="vc-c3">LOC</span>
        <span className="vc-c4">STG</span>
        <span className="vc-c5">SETS</span>
        <span className="vc-c6">T−</span>
      </div>

      {/* Table */}
      <div className="main vc-tbody">
        {visible.length === 0 && (
          <div className="vc-empty">
            <div>// no rows</div>
            <div className="vc-empty-sub">try <span className="vc-empty-cmd">/all</span> or clear filter</div>
          </div>
        )}
        {visible.map((f, i) => (
          <RowC
            key={f.id}
            fest={f}
            saved={!!saved[f.id]}
            selected={i === selected}
            onSelect={() => setSelected(i)}
            onToggle={() => setSaved(s => ({ ...s, [f.id]: !s[f.id] }))}
          />
        ))}
        <div style={{ height: 8 }}></div>
      </div>

      {/* Bottom status / nav bar */}
      <div className="vc-statusbar">
        <div className="vc-statusbar-tabs">
          <KeyC id="F1" label="fests"    active={tab} onClick={setTab} />
          <KeyC id="F2" label="schedule" active={tab} onClick={setTab} />
          <KeyC id="F3" label="now"      active={tab} onClick={setTab} dot />
          <KeyC id="F4" label="you"      active={tab} onClick={setTab} />
        </div>
        <div className="vc-statusbar-hints">
          <span><kbd>↑↓</kbd> nav</span>
          <span><kbd>↩</kbd> open</span>
          <span><kbd>★</kbd> save</span>
        </div>
      </div>
    </div>
  );
}

function ScopeC({ id, label, current, onClick, badge }) {
  const isActive = current === id;
  return (
    <button
      className={"vc-scope" + (isActive ? " active" : "")}
      onClick={() => onClick(id)}
    >
      <span className="vc-scope-slash">/</span>
      <span>{label}</span>
      {typeof badge === "number" && badge > 0 && (
        <span className="vc-scope-badge">{badge}</span>
      )}
    </button>
  );
}

function RowC({ fest, saved, selected, onSelect, onToggle }) {
  const [m1, d1] = fest.dateRange[0].split(" ");
  const [, d2]   = fest.dateRange[1].split(" ");
  const isPast = fest.status === "past";
  const isLive = fest.status === "live";

  // days-away bar (0-120 cap)
  const cap = 120;
  const pct = Math.max(0, Math.min(1, fest.daysAway / cap));
  const segs = 6;
  const filled = Math.round((1 - pct) * segs);

  return (
    <div
      className={"vc-row" + (selected ? " selected" : "") + (isPast ? " past" : "") + (isLive ? " live" : "")}
      onClick={onSelect}
    >
      <span className="vc-c0">
        <button
          className={"vc-star" + (saved ? " on" : "")}
          onClick={(e) => { e.stopPropagation(); onToggle(); }}
        >
          {saved ? "★" : "☆"}
        </button>
      </span>
      <span className="vc-c1">
        <span className="vc-row-name">{fest.name}</span>
        {isLive && <span className="vc-row-live">● LIVE</span>}
        <span className="vc-row-headliner">{fest.headliners[0] || "—"}</span>
      </span>
      <span className="vc-c2">
        <span className="vc-row-mono">{m1.slice(0,3)} {d1}</span>
        <span className="vc-row-arrow">→</span>
        <span className="vc-row-mono">{d2}</span>
      </span>
      <span className="vc-c3">
        <span className="vc-row-mono">{fest.cc}</span>
        <span className="vc-row-city">{fest.city.slice(0,3)}</span>
      </span>
      <span className="vc-c4 vc-row-num">{fest.stages || "—"}</span>
      <span className="vc-c5 vc-row-num">{fest.sets || "—"}</span>
      <span className="vc-c6">
        <span className="vc-bar">
          {Array.from({ length: segs }, (_, i) => (
            <span key={i} className={"vc-bar-seg" + (i < filled ? " on" : "")}></span>
          ))}
        </span>
        <span className={"vc-cd" + (isLive ? " live" : "") + (isPast ? " past" : "")}>
          {isLive ? "NOW" : isPast ? `+${Math.abs(fest.daysAway)}d` : `−${fest.daysAway}d`}
        </span>
      </span>
    </div>
  );
}

function KeyC({ id, label, active, onClick, dot }) {
  const isActive = active === id;
  return (
    <button
      className={"vc-key" + (isActive ? " active" : "")}
      onClick={() => onClick(id)}
    >
      <span className="vc-key-id">{id}</span>
      <span className="vc-key-label">{label}{dot && <span className="vc-key-dot"></span>}</span>
    </button>
  );
}

window.VariantC = VariantC;
