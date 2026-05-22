// OFFBEAT NowStripView — V6 Now Strip (departures board)
// Hero: live dot + "ON NOW · STAGE NAME", big artist name, time range
// Countdown: "T−HH:MM:SS" to next starred set (mono, accent, 28px)
// Departures board: grid header (TIME / ARTIST·STAGE / STATUS)
// Departure rows: time | 4px color bar | artist + meta | status badge

import 'dart:async';
import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/live_dot.dart';
import '../../widgets/dotted_border.dart';

class NowStripView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;

  const NowStripView({super.key, required this.sets, required this.stages});

  @override
  State<NowStripView> createState() => _NowStripViewState();
}

class _NowStripViewState extends State<NowStripView> {
  late Timer _timer;
  int _seconds = 28; // fake ticking seconds

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      setState(() {
        _seconds = (_seconds + 1) % 60;
      });
    });
  }

  @override
  void dispose() {
    _timer.cancel();
    super.dispose();
  }

  Map<String, Stage> get _stageById => {for (final s in widget.stages) s.id: s};

  /// Current day id based on real time (matches Day.id format).
  String get _todayId {
    const dow = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
    return dow[DateTime.now().weekday - 1];
  }

  /// Minutes since midnight right now.
  int get _nowMin {
    final now = DateTime.now();
    return now.hour * 60 + now.minute;
  }

  @override
  Widget build(BuildContext context) {
    final stageById = _stageById;
    final nowT = _nowMin;
    // Show all sets for today (best-effort match on day id)
    final sets = widget.sets.where((s) => s.day == _todayId).toList();
    final live = sets.cast<FestSet?>().firstWhere(
      (s) => s!.live,
      orElse: () => null,
    );
    final liveStage = live != null ? stageById[live.stage] : null;

    // Next 4 hours of sets
    final upcoming =
        sets.where((s) => s.t > nowT && s.t < nowT + 240).toList()
          ..sort((a, b) => a.t.compareTo(b.t));

    // Next starred set
    final nextStarred = sets.where((s) => s.starred && s.t > nowT).toList()
      ..sort((a, b) => a.t.compareTo(b.t));
    final next = nextStarred.isEmpty ? null : nextStarred.first;
    final diff = next != null ? next.t - nowT : 0;
    final hh = (diff ~/ 60).toString().padLeft(2, '0');
    final mm = (diff % 60).toString().padLeft(2, '0');
    final ss = _seconds.toString().padLeft(2, '0');

    return ListView(
      padding: EdgeInsets.zero,
      children: [
        // Hero — currently playing
        _NowHero(
          live: live,
          liveStage: liveStage,
          next: next,
          hh: hh,
          mm: mm,
          ss: ss,
        ),
        // Departures board header
        DottedBorder.bottom(
          child: Container(
            color: colorSurface1,
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 8),
            child: Row(
              children: const [
                SizedBox(width: 56, child: Text('TIME', style: _depHeadStyle)),
                SizedBox(width: 12),
                SizedBox(width: 4), // color bar
                SizedBox(width: 12),
                Expanded(child: Text('ARTIST · STAGE', style: _depHeadStyle)),
                SizedBox(
                  width: 70,
                  child: Text(
                    'STATUS',
                    textAlign: TextAlign.right,
                    style: _depHeadStyle,
                  ),
                ),
              ],
            ),
          ),
        ),
        // Departure rows
        ...upcoming.take(8).map((s) {
          final stage = stageById[s.stage]!;
          final inMin = s.t - nowT;
          String status;
          Color statusColor;

          if (s.starred) {
            status = '★ STARRED';
            statusColor = colorAccent;
          } else if (s.clashes.isNotEmpty) {
            status = '! CLASH';
            statusColor = colorWarn;
          } else if (inMin <= 15) {
            status = 'T−${inMin}M';
            statusColor = colorAccent;
          } else {
            status = 'QUEUED';
            statusColor = colorFg4;
          }

          return _DepartureRow(
            set: s,
            stage: stage,
            status: status,
            statusColor: statusColor,
          );
        }),
        const SizedBox(height: 90),
      ],
    );
  }

  static const _depHeadStyle = TextStyle(
    fontFamily: 'JetBrainsMono',
    fontSize: 9,
    fontWeight: FontWeight.w700,
    letterSpacing: 0.1 * 9,
    color: colorFg3,
    height: 1,
  );
}

class _NowHero extends StatelessWidget {
  final FestSet? live;
  final Stage? liveStage;
  final FestSet? next;
  final String hh;
  final String mm;
  final String ss;

  const _NowHero({
    required this.live,
    required this.liveStage,
    required this.next,
    required this.hh,
    required this.mm,
    required this.ss,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Container(
        decoration: BoxDecoration(
          gradient: RadialGradient(
            center: const Alignment(-0.64, -0.4),
            radius: 1.2,
            colors: [colorAccent.withValues(alpha: 0.16), Colors.transparent],
          ),
          color: colorBg,
        ),
        padding: const EdgeInsets.fromLTRB(18, 18, 18, 22),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // "ON NOW" label
            Row(
              children: [
                const LiveDot(size: 7),
                const SizedBox(width: 8),
                Text(
                  '// ON NOW · ${liveStage?.name ?? 'STAGE'}',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.1 * 10,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
              ],
            ),
            const SizedBox(height: 10),
            // Big artist name
            Text(
              live?.artist ?? 'LOADING…',
              style: const TextStyle(
                fontFamily: 'Helvetica',
                fontWeight: FontWeight.w700,
                fontSize: 30,
                letterSpacing: -0.02 * 30,
                height: 1,
                color: colorFg,
              ),
            ),
            const SizedBox(height: 6),
            // Time + genre
            if (live != null)
              Text(
                '${fmtTime(live!.t)} → ${fmtTime(live!.t + live!.dur)} · ${live!.genre}',
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  letterSpacing: 0.08 * 11,
                  color: colorFg2,
                  height: 1,
                ),
              ),
            // Countdown to next starred
            if (next != null) ...[
              const SizedBox(height: 14),
              Row(
                crossAxisAlignment: CrossAxisAlignment.baseline,
                textBaseline: TextBaseline.alphabetic,
                children: [
                  Text(
                    'T−$hh:$mm:$ss',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 28,
                      fontWeight: FontWeight.w500,
                      letterSpacing: -0.02 * 28,
                      color: colorAccent,
                      height: 1,
                    ),
                  ),
                  const SizedBox(width: 10),
                  Text(
                    'next ★ ${next!.artist}',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      letterSpacing: 0.08 * 10,
                      color: colorFg3,
                      height: 1,
                    ),
                  ),
                ],
              ),
            ],
          ],
        ),
      ),
    );
  }
}

class _DepartureRow extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final String status;
  final Color statusColor;

  const _DepartureRow({
    required this.set,
    required this.stage,
    required this.status,
    required this.statusColor,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: () {},
          splashColor: Colors.transparent,
          highlightColor: colorSurface1,
          child: Container(
            decoration: set.starred
                ? const BoxDecoration(
                    gradient: LinearGradient(
                      colors: [colorAccentWash, Colors.transparent],
                      stops: [0.0, 0.8],
                    ),
                  )
                : null,
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
            child: Row(
              children: [
                // Time (56px)
                SizedBox(
                  width: 56,
                  child: Text(
                    fmtTime(set.t),
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 14,
                      fontWeight: FontWeight.w500,
                      letterSpacing: -0.02 * 14,
                      color: colorFg,
                      height: 1,
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                // Color bar (4px)
                Container(width: 4, height: 28, color: Color(stage.color)),
                const SizedBox(width: 12),
                // Artist + meta
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        set.artist,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontFamily: 'Helvetica',
                          fontWeight: FontWeight.w700,
                          fontSize: 15,
                          letterSpacing: -0.01 * 15,
                          height: 1.1,
                          color: colorFg,
                        ),
                      ),
                      const SizedBox(height: 3),
                      Text(
                        '${stage.name} · ${set.dur}M · ${set.genre}',
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          letterSpacing: 0.08 * 9,
                          color: colorFg3,
                          height: 1,
                        ),
                      ),
                    ],
                  ),
                ),
                // Status
                SizedBox(
                  width: 70,
                  child: Text(
                    status,
                    textAlign: TextAlign.right,
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      letterSpacing: 0.08 * 10,
                      color: statusColor,
                      height: 1,
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
