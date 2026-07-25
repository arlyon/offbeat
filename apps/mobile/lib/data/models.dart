// OFFBEAT Models — Domain types for festival data

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
  final DateTime? startDate;
  final DateTime? endDate;
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
    this.startDate,
    this.endDate,
    required this.headliners,
  });

  /// Live status inferred from start/end dates.
  FestStatus get status {
    final now = DateTime.now();
    if (startDate != null && endDate != null) {
      if (now.isAfter(endDate!.add(const Duration(days: 1)))) {
        return FestStatus.past;
      }
      if (!now.isBefore(startDate!)) {
        return FestStatus.live;
      }
    }
    return FestStatus.upcoming;
  }

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
      sets: 0,
      saved: 0,
      hue: hue,
      genres: (j['genres'] as List<dynamic>?)?.cast<String>() ?? [],
      startDate: startDate,
      endDate: endDate,
      headliners: const [],
    );
  }

  static String _formatDateRange(DateTime start, DateTime end) {
    const months = [
      'JAN',
      'FEB',
      'MAR',
      'APR',
      'MAY',
      'JUN',
      'JUL',
      'AUG',
      'SEP',
      'OCT',
      'NOV',
      'DEC',
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

  factory Stage.fromJson(Map<String, dynamic> j) {
    final colorStr = j['color'] as String? ?? '#FF2D8F';
    final hex = colorStr.replaceFirst('#', '');
    final colorInt = int.parse('FF$hex', radix: 16);
    return Stage(
      id: j['id'] as String,
      name: j['name'] as String,
      short: j['short'] as String,
      color: colorInt,
    );
  }
}

class Day {
  final String id;
  final String label;
  final String dayNum;
  final String month;
  final int year;

  const Day({
    required this.id,
    required this.label,
    required this.dayNum,
    required this.month,
    required this.year,
  });

  factory Day.fromJson(Map<String, dynamic> j) {
    return Day(
      id: j['id'] as String,
      label: j['label'] as String,
      dayNum: (j['num']).toString(),
      month: j['month'] as String,
      year: (j['year'] as num?)?.toInt() ?? 0,
    );
  }
}

class FestSet {
  final String id;
  final String day;
  final String stage;
  final String artist;
  final int t; // minutes from midnight
  final int dur; // minutes
  final String genre;
  bool starred;
  final bool live;
  final bool cancelled;
  final List<String> clashes;

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
    this.cancelled = false,
    this.clashes = const [],
  });

  factory FestSet.fromJson(Map<String, dynamic> j) {
    return FestSet(
      id: j['id'] as String,
      day: j['day'] as String,
      stage: j['stage'] as String,
      artist: j['artist'] as String,
      t: (j['startMin'] as num).toInt(),
      dur: (j['durationMin'] as num).toInt(),
      genre: (j['genre'] as String?) ?? '',
      starred: false,
      live: false,
      cancelled: (j['cancelled'] as bool?) ?? false,
      clashes: const [],
    );
  }

  FestSet copyWith({bool? starred, int? t, List<String>? clashes}) => FestSet(
    id: id,
    day: day,
    stage: stage,
    artist: artist,
    t: t ?? this.t,
    dur: dur,
    genre: genre,
    starred: starred ?? this.starred,
    live: live,
    cancelled: cancelled,
    clashes: clashes ?? this.clashes,
  );
}

/// Derive every set's overlaps with the user's currently liked schedule.
/// A liked set only clashes with other liked sets; an unliked set is marked
/// when it overlaps a liked set, allowing "hide clashes" filtering.
List<FestSet> withScheduleClashes(List<FestSet> sets) {
  final clashesBySet = {for (final set in sets) set.id: <String>{}};
  final liked = sets.where((set) => set.starred && !set.cancelled).toList();

  for (final set in sets) {
    if (set.cancelled || set.dur <= 0) continue;
    for (final likedSet in liked) {
      if (set.id == likedSet.id || !_setsOverlap(set, likedSet)) continue;
      clashesBySet[set.id]!.add(likedSet.id);
    }
  }

  return sets.map((set) {
    final clashes = clashesBySet[set.id]!.toList()..sort();
    return set.copyWith(clashes: clashes);
  }).toList();
}

bool _setsOverlap(FestSet a, FestSet b) {
  if (a.day != b.day || a.cancelled || b.cancelled) return false;
  return a.t < b.t + b.dur && b.t < a.t + a.dur;
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
