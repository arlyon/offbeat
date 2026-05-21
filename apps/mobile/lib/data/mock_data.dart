// OFFBEAT Mock Data
// Translated from docs/designs/lineup/project/shared.jsx
// and docs/designs/lineup-festival-view/project/data.jsx

// ── Models ────────────────────────────────────────────────────

class Festival {
  final String id;
  final String name;
  final String year;
  final String where;
  final String city;
  final String cc;
  final String dates;
  final List<String> dateRange;
  final int daysAway;
  final int stages;
  final int sets;
  final int saved;
  final int hue;
  final List<String> genres;
  final FestStatus status;
  final List<String> headliners;

  const Festival({
    required this.id,
    required this.name,
    required this.year,
    required this.where,
    required this.city,
    required this.cc,
    required this.dates,
    required this.dateRange,
    required this.daysAway,
    required this.stages,
    required this.sets,
    required this.saved,
    required this.hue,
    required this.genres,
    required this.status,
    required this.headliners,
  });

  /// Parse a festival from the server's JSON response.
  factory Festival.fromJson(Map<String, dynamic> j) {
    final startDate = DateTime.tryParse(j['startDate'] as String? ?? '');
    final endDate = DateTime.tryParse(j['endDate'] as String? ?? '');
    final now = DateTime.now();

    // Build human-readable date range string
    String dates = '';
    List<String> dateRange = [];
    int daysAway = 0;
    if (startDate != null && endDate != null) {
      dates = _formatDateRange(startDate, endDate);
      dateRange = _buildDateRange(startDate, endDate);
      daysAway = startDate.difference(now).inDays;
    }

    // Parse status
    final statusStr = (j['status'] as String?) ?? 'upcoming';
    FestStatus status;
    if (statusStr == 'live') {
      status = FestStatus.live;
    } else if (statusStr == 'past') {
      status = FestStatus.past;
    } else {
      status = FestStatus.upcoming;
    }

    // Derive a hue from the festival ID for the art tile
    int hue = 0;
    for (final c in (j['id'] as String).codeUnits) {
      hue = (hue + c) % 360;
    }

    final stages = j['stages'] as List<dynamic>? ?? [];

    return Festival(
      id: j['id'] as String,
      name: j['name'] as String,
      year: (j['year'] ?? '').toString(),
      where: j['location'] as String? ?? '',
      city: j['city'] as String? ?? '',
      cc: j['country'] as String? ?? '',
      dates: dates,
      dateRange: dateRange,
      daysAway: daysAway,
      stages: stages.length,
      sets: 0, // sets count comes from the lineup endpoint
      saved: 0,
      hue: hue,
      genres: (j['genres'] as List<dynamic>?)?.cast<String>() ?? [],
      status: status,
      headliners: const [], // headliners come from the lineup endpoint
    );
  }

  static String _formatDateRange(DateTime start, DateTime end) {
    const months = [
      'JAN', 'FEB', 'MAR', 'APR', 'MAY', 'JUN',
      'JUL', 'AUG', 'SEP', 'OCT', 'NOV', 'DEC',
    ];
    if (start.month == end.month) {
      return '${start.day}–${end.day} ${months[start.month - 1]} ${start.year}';
    }
    return '${start.day} ${months[start.month - 1]}–${end.day} ${months[end.month - 1]} ${start.year}';
  }

  static List<String> _buildDateRange(DateTime start, DateTime end) {
    final days = <String>[];
    var d = start;
    while (!d.isAfter(end)) {
      days.add(d.toIso8601String().split('T')[0]);
      d = d.add(const Duration(days: 1));
    }
    return days;
  }
}

enum FestStatus { live, upcoming, past }

class Stage {
  final String id;
  final String name;
  final String short;
  final int color; // 0xFFRRGGBB

  const Stage({
    required this.id,
    required this.name,
    required this.short,
    required this.color,
  });
}

class Day {
  final String id;
  final String label;
  final String num;
  final String month;

  const Day({
    required this.id,
    required this.label,
    required this.num,
    required this.month,
  });
}

class FestSet {
  final int id;
  final String day;
  final String stage;
  final String artist;
  final int t; // minutes from midnight
  final int dur; // minutes
  final String genre;
  bool starred;
  final bool live;
  final List<int> clashes;

  FestSet({
    required this.id,
    required this.day,
    required this.stage,
    required this.artist,
    required this.t,
    required this.dur,
    required this.genre,
    this.starred = false,
    this.live = false,
    this.clashes = const [],
  });

  FestSet copyWith({bool? starred}) => FestSet(
    id: id,
    day: day,
    stage: stage,
    artist: artist,
    t: t,
    dur: dur,
    genre: genre,
    starred: starred ?? this.starred,
    live: live,
    clashes: clashes,
  );
}

// ── Helpers ───────────────────────────────────────────────────

String fmtTime(int mins) {
  final h = (mins ~/ 60) % 24;
  final m = mins % 60;
  return '${h.toString().padLeft(2, '0')}:${m.toString().padLeft(2, '0')}';
}

String fmtCountdown(int days) {
  if (days == 0) return 'TODAY';
  if (days < 0) return '${days.abs()}D AGO';
  if (days < 10) return 'T−${days.toString().padLeft(2, '0')}D';
  return 'T−${days}D';
}

// ── Data ──────────────────────────────────────────────────────

const List<Festival> kFests = [
  Festival(
    id: 'fieldday26',
    name: 'Field Day',
    year: '2026',
    where: 'Brockwell Park, London',
    city: 'LONDON',
    cc: 'UK',
    dates: 'Aug 22 — 23',
    dateRange: ['AUG 22', 'AUG 23'],
    daysAway: 4,
    stages: 6,
    sets: 64,
    saved: 12,
    hue: 1,
    genres: ['ELECTRONIC', 'HOUSE'],
    status: FestStatus.live,
    headliners: ['Four Tet', 'Bicep', 'Floating Points'],
  ),
  Festival(
    id: 'primavera26',
    name: 'Primavera Pro',
    year: '2026',
    where: 'Parc del Fòrum, Barcelona',
    city: 'BARCELONA',
    cc: 'ES',
    dates: 'Jun 04 — 08',
    dateRange: ['JUN 04', 'JUN 08'],
    daysAway: 32,
    stages: 9,
    sets: 142,
    saved: 24,
    hue: 2,
    genres: ['INDIE', 'ELECTRONIC'],
    status: FestStatus.upcoming,
    headliners: ['Charli XCX', 'Mount Kimbie', 'Yves Tumor'],
  ),
  Festival(
    id: 'draaimolen',
    name: 'Draaimolen',
    year: '2026',
    where: 'Tilburg, NL',
    city: 'TILBURG',
    cc: 'NL',
    dates: 'Sep 19 — 20',
    dateRange: ['SEP 19', 'SEP 20'],
    daysAway: 119,
    stages: 4,
    sets: 38,
    saved: 0,
    hue: 3,
    genres: ['TECHNO'],
    status: FestStatus.upcoming,
    headliners: ['DVS1', 'Helena Hauff', 'Stenny'],
  ),
  Festival(
    id: 'houghton',
    name: 'Houghton',
    year: '2026',
    where: 'Houghton Hall, Norfolk',
    city: 'NORFOLK',
    cc: 'UK',
    dates: 'Aug 06 — 10',
    dateRange: ['AUG 06', 'AUG 10'],
    daysAway: 78,
    stages: 7,
    sets: 89,
    saved: 0,
    hue: 4,
    genres: ['ELECTRONIC', '24H'],
    status: FestStatus.upcoming,
    headliners: ['Craig Richards', 'Ben UFO', 'Move D'],
  ),
  Festival(
    id: 'dekmantel',
    name: 'Dekmantel',
    year: '2026',
    where: 'Amsterdamse Bos, NL',
    city: 'AMSTERDAM',
    cc: 'NL',
    dates: 'Aug 05 — 09',
    dateRange: ['AUG 05', 'AUG 09'],
    daysAway: 77,
    stages: 8,
    sets: 110,
    saved: 0,
    hue: 5,
    genres: ['HOUSE', 'TECHNO'],
    status: FestStatus.upcoming,
    headliners: ['Honey Dijon', 'DJ Stingray', 'Carista'],
  ),
  Festival(
    id: 'berlinatonal',
    name: 'Atonal',
    year: '2026',
    where: 'Kraftwerk, Berlin',
    city: 'BERLIN',
    cc: 'DE',
    dates: 'Aug 27 — 31',
    dateRange: ['AUG 27', 'AUG 31'],
    daysAway: 99,
    stages: 5,
    sets: 72,
    saved: 0,
    hue: 1,
    genres: ['AMBIENT', 'INDUSTRIAL'],
    status: FestStatus.upcoming,
    headliners: ['Lyra Pramuk', 'Ben Frost', 'Pole'],
  ),
  Festival(
    id: 'ade25',
    name: 'ADE',
    year: '2025',
    where: 'Amsterdam, NL',
    city: 'AMSTERDAM',
    cc: 'NL',
    dates: 'Oct 16 — 20 · 2025',
    dateRange: ['OCT 16', 'OCT 20'],
    daysAway: -218,
    stages: 0,
    sets: 0,
    saved: 7,
    hue: 2,
    genres: ['CONF', 'ELECTRONIC'],
    status: FestStatus.past,
    headliners: [],
  ),
];

const List<Stage> kStages = [
  Stage(id: 's1', name: 'STAGE 1', short: 'S1', color: 0xFFFF2D8F),
  Stage(id: 's2', name: 'STAGE 2', short: 'S2', color: 0xFF3DDBD9),
  Stage(id: 's3', name: 'RED ROOM', short: 'RR', color: 0xFFFFB347),
  Stage(id: 's4', name: 'STAGE 4', short: 'S4', color: 0xFF9BE15D),
  Stage(id: 's5', name: 'BARN', short: 'BN', color: 0xFFC77DFF),
  Stage(id: 's6', name: 'OUTPOST', short: 'OP', color: 0xFFFF8C42),
];

const List<Day> kDays = [
  Day(id: 'fri', label: 'FRI', num: '22', month: 'AUG'),
  Day(id: 'sat', label: 'SAT', num: '23', month: 'AUG'),
];

List<FestSet> buildSets() => [
  // ── FRIDAY ─────────────────────────────────────────────────
  FestSet(
    id: 1,
    day: 'fri',
    stage: 's1',
    artist: 'Floating Points',
    t: 18 * 60,
    dur: 90,
    genre: 'ELECTRONIC',
  ),
  FestSet(
    id: 2,
    day: 'fri',
    stage: 's1',
    artist: 'Four Tet',
    t: 20 * 60,
    dur: 80,
    genre: 'ELECTRONIC',
    live: true,
    starred: true,
  ),
  FestSet(
    id: 3,
    day: 'fri',
    stage: 's1',
    artist: 'Caribou',
    t: 21 * 60 + 30,
    dur: 90,
    genre: 'ELECTRONIC',
    starred: true,
    clashes: [6, 9],
  ),
  FestSet(
    id: 4,
    day: 'fri',
    stage: 's1',
    artist: 'Aphex Twin',
    t: 23 * 60 + 30,
    dur: 90,
    genre: 'ELECTRONIC',
  ),
  FestSet(
    id: 5,
    day: 'fri',
    stage: 's1',
    artist: 'Jamie xx',
    t: 25 * 60 + 30,
    dur: 60,
    genre: 'ELECTRONIC',
    starred: true,
  ),

  FestSet(
    id: 6,
    day: 'fri',
    stage: 's2',
    artist: 'Overmono',
    t: 19 * 60,
    dur: 75,
    genre: 'LIVE',
    starred: true,
  ),
  FestSet(
    id: 7,
    day: 'fri',
    stage: 's2',
    artist: 'Bicep',
    t: 21 * 60,
    dur: 90,
    genre: 'LIVE',
    starred: true,
    clashes: [3],
  ),
  FestSet(
    id: 8,
    day: 'fri',
    stage: 's2',
    artist: 'Romy',
    t: 23 * 60,
    dur: 60,
    genre: 'LIVE',
  ),
  FestSet(
    id: 9,
    day: 'fri',
    stage: 's2',
    artist: 'Bonobo b2b Ross',
    t: 24 * 60,
    dur: 120,
    genre: 'LIVE',
    clashes: [3],
  ),

  FestSet(
    id: 10,
    day: 'fri',
    stage: 's3',
    artist: 'Sherelle',
    t: 19 * 60 + 30,
    dur: 60,
    genre: 'JUNGLE',
  ),
  FestSet(
    id: 11,
    day: 'fri',
    stage: 's3',
    artist: 'Helena Hauff',
    t: 21 * 60,
    dur: 60,
    genre: 'TECHNO',
    starred: true,
    clashes: [7],
  ),
  FestSet(
    id: 12,
    day: 'fri',
    stage: 's3',
    artist: 'ANNA',
    t: 22 * 60 + 30,
    dur: 90,
    genre: 'TECHNO',
  ),
  FestSet(
    id: 13,
    day: 'fri',
    stage: 's3',
    artist: 'SPFDJ',
    t: 24 * 60,
    dur: 90,
    genre: 'TECHNO',
  ),
  FestSet(
    id: 14,
    day: 'fri',
    stage: 's3',
    artist: 'Adam Beyer',
    t: 25 * 60 + 30,
    dur: 90,
    genre: 'TECHNO',
  ),

  FestSet(
    id: 15,
    day: 'fri',
    stage: 's4',
    artist: 'DJ Storm',
    t: 18 * 60 + 30,
    dur: 60,
    genre: 'JUNGLE',
  ),
  FestSet(
    id: 16,
    day: 'fri',
    stage: 's4',
    artist: 'Sub Focus DJ',
    t: 20 * 60,
    dur: 75,
    genre: 'D&B',
  ),
  FestSet(
    id: 17,
    day: 'fri',
    stage: 's4',
    artist: 'Goldie',
    t: 21 * 60 + 30,
    dur: 90,
    genre: 'D&B',
  ),
  FestSet(
    id: 18,
    day: 'fri',
    stage: 's4',
    artist: 'Sully',
    t: 23 * 60 + 15,
    dur: 75,
    genre: 'D&B',
  ),
  FestSet(
    id: 19,
    day: 'fri',
    stage: 's4',
    artist: 'Tim Reaper',
    t: 25 * 60,
    dur: 90,
    genre: 'JUNGLE',
  ),

  FestSet(
    id: 20,
    day: 'fri',
    stage: 's5',
    artist: 'Skee Mask',
    t: 19 * 60,
    dur: 90,
    genre: 'BREAKS',
  ),
  FestSet(
    id: 21,
    day: 'fri',
    stage: 's5',
    artist: 'Joy Orbison',
    t: 21 * 60 + 30,
    dur: 60,
    genre: 'HOUSE',
    starred: true,
  ),
  FestSet(
    id: 22,
    day: 'fri',
    stage: 's5',
    artist: 'Ben UFO',
    t: 23 * 60,
    dur: 90,
    genre: 'HOUSE',
  ),
  FestSet(
    id: 23,
    day: 'fri',
    stage: 's5',
    artist: 'Hessle Audio',
    t: 25 * 60,
    dur: 120,
    genre: 'HOUSE',
  ),

  FestSet(
    id: 24,
    day: 'fri',
    stage: 's6',
    artist: 'DJ Python',
    t: 19 * 60 + 30,
    dur: 90,
    genre: 'DUB',
  ),
  FestSet(
    id: 25,
    day: 'fri',
    stage: 's6',
    artist: 'Object Blue',
    t: 21 * 60 + 30,
    dur: 60,
    genre: 'EXPERIMENTAL',
  ),
  FestSet(
    id: 26,
    day: 'fri',
    stage: 's6',
    artist: 'Lord Apex',
    t: 23 * 60,
    dur: 60,
    genre: 'HIP-HOP',
  ),
  FestSet(
    id: 27,
    day: 'fri',
    stage: 's6',
    artist: 'DJ Marfox',
    t: 24 * 60 + 30,
    dur: 90,
    genre: 'GLOBAL',
  ),

  // ── SATURDAY ──────────────────────────────────────────────
  FestSet(
    id: 40,
    day: 'sat',
    stage: 's1',
    artist: 'Peggy Gou',
    t: 21 * 60,
    dur: 75,
    genre: 'HOUSE',
    starred: true,
  ),
  FestSet(
    id: 41,
    day: 'sat',
    stage: 's1',
    artist: 'Burial',
    t: 23 * 60,
    dur: 90,
    genre: 'ELECTRONIC',
  ),
  FestSet(
    id: 42,
    day: 'sat',
    stage: 's2',
    artist: 'Skee Mask',
    t: 22 * 60,
    dur: 90,
    genre: 'TECHNO',
    starred: true,
  ),
  FestSet(
    id: 43,
    day: 'sat',
    stage: 's3',
    artist: 'Tama Sumo',
    t: 20 * 60,
    dur: 120,
    genre: 'HOUSE',
  ),
  FestSet(
    id: 44,
    day: 'sat',
    stage: 's4',
    artist: 'Loraine James',
    t: 21 * 60 + 30,
    dur: 60,
    genre: 'ELECTRONIC',
  ),
  FestSet(
    id: 45,
    day: 'sat',
    stage: 's5',
    artist: 'Nala Sinephro',
    t: 19 * 60,
    dur: 60,
    genre: 'AMBIENT',
  ),
];

const List<String> kGenres = [
  'ELECTRONIC',
  'LIVE',
  'TECHNO',
  'D&B',
  'JUNGLE',
  'HOUSE',
  'BREAKS',
  'DUB',
  'HIP-HOP',
  'AMBIENT',
  'EXPERIMENTAL',
  'GLOBAL',
];

// "now" — pin our fake time to 20:30 on FRI so the live state is realistic
const String kNowDay = 'fri';
const int kNowT = 20 * 60 + 30; // 20:30
