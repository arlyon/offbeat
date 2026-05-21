/* Festival Views — shared bits used by every variant */

const { useState, useEffect, useRef, useMemo, useLayoutEffect, useCallback } = React;

/* Lucide-icon wrapper */
function Icon({ name, size = 18, stroke = 1.5, color = "currentColor", className = "" }) {
  const ref = useRef(null);
  useEffect(() => {
    if (!ref.current || !window.lucide) return;
    ref.current.innerHTML = "";
    const ic = window.lucide.icons[name] || window.lucide.icons.Square;
    const svg = window.lucide.createElement(ic);
    svg.setAttribute("width", size);
    svg.setAttribute("height", size);
    svg.setAttribute("stroke", color);
    svg.setAttribute("stroke-width", stroke);
    ref.current.appendChild(svg);
  }, [name, size, stroke, color]);
  return <span ref={ref} className={"lc " + className} style={{ display: "inline-flex", lineHeight: 0 }} />;
}

/* Equalizer mark */
function Mark() {
  return (
    <span className="mark-eq" aria-label="OFFBEAT">
      <span></span><span></span><span></span>
    </span>
  );
}

/* iOS status strip */
function StatusBar({ time = "20:30" }) {
  return (
    <div className="statusbar">
      <span>{time}</span>
      <span className="sys">
        <span style={{ letterSpacing: "0.05em" }}>•••</span>
        <span>OFFBEAT</span>
        <span style={{ color: "var(--fg-3)" }}>87%</span>
      </span>
    </div>
  );
}

/* Top nav — wordmark left, search/filter right.
   showBack = true → swap mark for chevron */
function TopNav({ festival, right, showBack = false, onBack }) {
  return (
    <div className="topnav">
      <div className="wm">
        {showBack ? (
          <button className="icon-btn" onClick={onBack} style={{ marginLeft: -8 }}>
            <Icon name="ChevronLeft" size={18} />
          </button>
        ) : <Mark />}
        <span style={{ whiteSpace: "nowrap" }}>OFFBEAT<span className="accent">//</span></span>
        {festival && <>
          <span style={{ width: 1, height: 14, background: "var(--hairline)", margin: "0 4px", flexShrink: 0 }}></span>
          <span className="fest">{festival}</span>
        </>}
      </div>
      <div style={{ display: "flex", gap: 4 }}>
        {right}
      </div>
    </div>
  );
}

/* Standard nav-right cluster — search + filter */
function NavRightStandard({ onSearch, onFilter, filterCount = 0 }) {
  return (
    <>
      <button className="icon-btn" onClick={onSearch}><Icon name="Search" size={17} /></button>
      <button className="icon-btn" onClick={onFilter} style={{ position: "relative" }}>
        <Icon name="SlidersHorizontal" size={17} />
        {filterCount > 0 && (
          <span style={{
            position: "absolute", top: 4, right: 4,
            width: 14, height: 14,
            fontFamily: "var(--font-mono)", fontSize: 9, fontWeight: 700,
            background: "var(--accent)", color: "var(--accent-ink)",
            display: "flex", alignItems: "center", justifyContent: "center",
          }}>{filterCount}</span>
        )}
      </button>
    </>
  );
}

/* Caption strip at the bottom of an artboard, naming the variant */
function Caption({ name, desc }) {
  return (
    <div className="variant-caption">
      <div className="name">{name}</div>
      <div className="desc">{desc}</div>
    </div>
  );
}

/* A SetRow component reused across variants */
function SetRow({ set, stage, onOpen, onStar }) {
  const isLive = set.live;
  const clash = set.clashes && set.clashes.length;
  return (
    <button className={"set-row" + (isLive ? " live" : "")} onClick={onOpen}>
      <div className="time">
        {fmtTime(set.t)}
        <span className="end">→ {fmtTime(set.t + set.dur)}</span>
      </div>
      <div className="bar" style={{ background: stage.color }}></div>
      <div style={{ minWidth: 0 }}>
        <div className="name">
          {isLive && <span className="live-dot-mini"></span>}
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{set.artist}</span>
        </div>
        <div className={"sub" + (clash ? " warn" : "")}>
          {clash
            ? <>! CLASH × {set.clashes.length} STARRED</>
            : <>{stage.name} <span style={{ color: "var(--fg-4)" }}>|</span> {set.dur} MIN <span style={{ color: "var(--fg-4)" }}>|</span> {set.genre}</>}
        </div>
      </div>
      <span
        className={"star" + (set.starred ? " on" : "")}
        onClick={(e) => { e.stopPropagation(); onStar && onStar(set.id); }}
      >{set.starred ? "★" : "☆"}</span>
    </button>
  );
}

Object.assign(window, { Icon, Mark, StatusBar, TopNav, NavRightStandard, Caption, SetRow });
