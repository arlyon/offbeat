/* Festival sample data — Field Day 2026 (expanded for the gantt to look full).
   t = minutes from midnight (so 19:30 = 19*60+30 = 1170)
   dur = minutes
   ──────────────────────────────────────────────────────────── */

window.FESTIVAL = {
  name: "Field Day 2026",
  where: "Brockwell Park · London",
  edition: "EDITION 19",
};

window.STAGES = [
  { id: "s1", name: "STAGE 1",  short: "S1", color: "#FF2D8F" },
  { id: "s2", name: "STAGE 2",  short: "S2", color: "#3DDBD9" },
  { id: "s3", name: "RED ROOM", short: "RR", color: "#FFB347" },
  { id: "s4", name: "STAGE 4",  short: "S4", color: "#9BE15D" },
  { id: "s5", name: "BARN",     short: "BN", color: "#C77DFF" },
  { id: "s6", name: "OUTPOST",  short: "OP", color: "#FF8C42" },
];

window.DAYS = [
  { id: "fri", label: "FRI", num: "22", month: "AUG" },
  { id: "sat", label: "SAT", num: "23", month: "AUG" },
];

const S = (id, day, stage, artist, t, dur, genre, opts={}) =>
  ({ id, day, stage, artist, t, dur, genre, starred: false, live: false, ...opts });

window.SETS = [
  // ── FRIDAY ─────────────────────────────────────────────────
  S(1,  "fri","s1","Floating Points",   18*60,     90, "ELECTRONIC"),
  S(2,  "fri","s1","Four Tet",          20*60,     80, "ELECTRONIC", { live:true, starred:true }),
  S(3,  "fri","s1","Caribou",           21*60+30,  90, "ELECTRONIC", { starred:true, clashes:[6,9] }),
  S(4,  "fri","s1","Aphex Twin",        23*60+30,  90, "ELECTRONIC"),
  S(5,  "fri","s1","Jamie xx",          25*60+30,  60, "ELECTRONIC", { starred:true }),

  S(6,  "fri","s2","Overmono",          19*60,     75, "LIVE", { starred:true }),
  S(7,  "fri","s2","Bicep",             21*60,     90, "LIVE",  { starred:true, clashes:[3] }),
  S(8,  "fri","s2","Romy",              23*60,     60, "LIVE"),
  S(9,  "fri","s2","Bonobo b2b Ross",   24*60,     120,"LIVE", { clashes:[3] }),

  S(10, "fri","s3","Sherelle",          19*60+30,  60, "JUNGLE"),
  S(11, "fri","s3","Helena Hauff",      21*60,     60, "TECHNO", { starred:true, clashes:[7] }),
  S(12, "fri","s3","ANNA",              22*60+30,  90, "TECHNO"),
  S(13, "fri","s3","SPFDJ",             24*60,     90, "TECHNO"),
  S(14, "fri","s3","Adam Beyer",        25*60+30,  90, "TECHNO"),

  S(15, "fri","s4","DJ Storm",          18*60+30,  60, "JUNGLE"),
  S(16, "fri","s4","Sub Focus DJ",      20*60,     75, "D&B"),
  S(17, "fri","s4","Goldie",            21*60+30,  90, "D&B"),
  S(18, "fri","s4","Sully",             23*60+15,  75, "D&B"),
  S(19, "fri","s4","Tim Reaper",        25*60,     90, "JUNGLE"),

  S(20, "fri","s5","Skee Mask",         19*60,     90, "BREAKS"),
  S(21, "fri","s5","Joy Orbison",       21*60+30,  60, "HOUSE", { starred:true }),
  S(22, "fri","s5","Ben UFO",           23*60,     90, "HOUSE"),
  S(23, "fri","s5","Hessle Audio",      25*60,     120,"HOUSE"),

  S(24, "fri","s6","DJ Python",         19*60+30,  90, "DUB"),
  S(25, "fri","s6","Object Blue",       21*60+30,  60, "EXPERIMENTAL"),
  S(26, "fri","s6","Lord Apex",         23*60,     60, "HIP-HOP"),
  S(27, "fri","s6","DJ Marfox",         24*60+30,  90, "GLOBAL"),

  // ── SATURDAY ──────────────────────────────────────────────
  S(40, "sat","s1","Peggy Gou",         21*60,     75, "HOUSE",  { starred:true }),
  S(41, "sat","s1","Burial",            23*60,     90, "ELECTRONIC"),
  S(42, "sat","s2","Skee Mask",         22*60,     90, "TECHNO", { starred:true }),
  S(43, "sat","s3","Tama Sumo",         20*60,     120,"HOUSE"),
  S(44, "sat","s4","Loraine James",     21*60+30,  60, "ELECTRONIC"),
  S(45, "sat","s5","Nala Sinephro",     19*60,     60, "AMBIENT"),
];

window.GENRES = ["ELECTRONIC","LIVE","TECHNO","D&B","JUNGLE","HOUSE","BREAKS","DUB","HIP-HOP","AMBIENT","EXPERIMENTAL","GLOBAL"];

// utility — minutes-of-day → "HH:MM"
window.fmtTime = (mins) => {
  const h = Math.floor(mins / 60) % 24;
  const m = mins % 60;
  return String(h).padStart(2, "0") + ":" + String(m).padStart(2, "0");
};

// "now" — pin our fake time to 20:30 on FRI so the live state is realistic
window.NOW = { day: "fri", t: 20*60 + 30 };
