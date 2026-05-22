// OFFBEAT ClashRadarView — V5 Clash Radar
// Hero: "X clashes in your night."
// Mini stage-lane strip diagram with starred set blobs + hatched clash zones
// Legend: scheduled / starred / clash
// Clash cards: warn-colored dotted border, A vs B options, resolve actions

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/chip.dart';

class ClashRadarView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;

  const ClashRadarView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
  });

  @override
  State<ClashRadarView> createState() => _ClashRadarViewState();
}

class _ClashRadarViewState extends State<ClashRadarView> {
  late String _day;

  @override
  void initState() {
    super.initState();
    _day = widget.days.first.id;
  }

  Map<String, Stage> get _stageById => {for (final s in widget.stages) s.id: s};

  List<FestSet> get _starred =>
      widget.sets.where((s) => s.day == _day && s.starred).toList();

  List<List<FestSet>> get _clashPairs {
    final starred = _starred;
    final pairs = <List<FestSet>>[];
    for (int i = 0; i < starred.length; i++) {
      for (int j = i + 1; j < starred.length; j++) {
        final a = starred[i], b = starred[j];
        if (a.t < b.t + b.dur && b.t < a.t + a.dur) {
          pairs.add([a, b]);
        }
      }
    }
    return pairs;
  }

  Day get _currentDay => widget.days.firstWhere((d) => d.id == _day);

  @override
  Widget build(BuildContext context) {
    final stageById = _stageById;
    final clashPairs = _clashPairs;
    final starred = _starred;
    final daySets = widget.sets.where((s) => s.day == _day).toList();
    final day = _currentDay;

    // Window for strip
    const wStart = 19 * 60;
    const wEnd = 25 * 60;
    const wRange = wEnd - wStart;
    double xPct(int m) => ((m - wStart) / wRange).clamp(0.0, 1.0);

    // Clash zones
    final clashZones = clashPairs.map((pair) {
      final a = pair[0], b = pair[1];
      return (
        start: [a.t, b.t].reduce((v, e) => v > e ? v : e),
        end: [a.t + a.dur, b.t + b.dur].reduce((v, e) => v < e ? v : e),
      );
    }).toList();

    // Stages with starred sets
    final stagesWithStars = widget.stages
        .where((st) => starred.any((s) => s.stage == st.id))
        .toList();

    return ListView(
      padding: EdgeInsets.zero,
      children: [
        // Hero
        DottedBorder.bottom(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(18, 18, 18, 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '// YOUR PLAN · ${day.label} ${day.dayNum} ${day.month}',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    letterSpacing: 0.08 * 11,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
                const SizedBox(height: 6),
                RichText(
                  text: TextSpan(
                    style: const TextStyle(
                      fontFamily: 'Helvetica',
                      fontWeight: FontWeight.w700,
                      fontSize: 32,
                      letterSpacing: -0.02 * 32,
                      height: 1.1,
                      color: colorFg,
                    ),
                    children: [
                      TextSpan(text: '${clashPairs.length} '),
                      TextSpan(
                        text: 'clash${clashPairs.length == 1 ? '' : 'es'}',
                        style: const TextStyle(color: colorAccent),
                      ),
                      const TextSpan(text: '\nin your night.'),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        // Strip diagram
        DottedBorder.bottom(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(18, 14, 18, 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // Axis labels
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children:
                      [
                            wStart,
                            wStart + wRange ~/ 4,
                            wStart + wRange ~/ 2,
                            wStart + (wRange * 3) ~/ 4,
                            wEnd,
                          ]
                          .map(
                            (m) => Text(
                              fmtTime(m),
                              style: const TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 9,
                                color: colorFg4,
                                letterSpacing: 0.05 * 9,
                                height: 1,
                              ),
                            ),
                          )
                          .toList(),
                ),
                const SizedBox(height: 6),
                // Stage lanes
                LayoutBuilder(
                  builder: (context, constraints) {
                    final w = constraints.maxWidth - 32; // minus label offset
                    return Column(
                      children: [
                        Container(
                          decoration: const BoxDecoration(
                            border: Border(
                              top: BorderSide(color: colorDotted, width: 1.5),
                              bottom: BorderSide(
                                color: colorDotted,
                                width: 1.5,
                              ),
                            ),
                          ),
                          child: Stack(
                            children: [
                              // Lane rows
                              Column(
                                children: stagesWithStars.map((stage) {
                                  final stageSets = daySets
                                      .where((s) => s.stage == stage.id)
                                      .toList();
                                  return SizedBox(
                                    height: 26,
                                    child: Stack(
                                      clipBehavior: Clip.hardEdge,
                                      children: [
                                        // Label
                                        Positioned(
                                          left: 0,
                                          top: 6,
                                          child: Text(
                                            stage.short,
                                            style: const TextStyle(
                                              fontFamily: 'JetBrainsMono',
                                              fontSize: 9,
                                              color: colorFg3,
                                              letterSpacing: 0.08 * 9,
                                              height: 1,
                                            ),
                                          ),
                                        ),
                                        // Set blobs
                                        ...stageSets.map((s) {
                                          final left = xPct(s.t) * w + 32;
                                          final right =
                                              xPct(s.t + s.dur) * w + 32;
                                          final width = right - left;
                                          if (width <= 0) {
                                            return const SizedBox.shrink();
                                          }
                                          return Positioned(
                                            left: left,
                                            top: 3,
                                            bottom: 3,
                                            width: width,
                                            child: Container(
                                              decoration: BoxDecoration(
                                                color: s.starred
                                                    ? colorAccentWash
                                                    : colorSurface2,
                                                border: Border(
                                                  left: BorderSide(
                                                    color: Color(stage.color),
                                                    width: 2,
                                                  ),
                                                ),
                                              ),
                                              padding:
                                                  const EdgeInsets.symmetric(
                                                    horizontal: 4,
                                                  ),
                                              child: Text(
                                                '${s.starred ? '★ ' : ''}${s.artist.split(' ').first}',
                                                maxLines: 1,
                                                overflow: TextOverflow.clip,
                                                style: const TextStyle(
                                                  fontFamily: 'JetBrainsMono',
                                                  fontSize: 9,
                                                  fontWeight: FontWeight.w700,
                                                  color: colorFg,
                                                  height: 1,
                                                ),
                                              ),
                                            ),
                                          );
                                        }),
                                        // Bottom divider
                                        const Positioned(
                                          bottom: 0,
                                          left: 0,
                                          right: 0,
                                          child: Divider(
                                            height: 1,
                                            thickness: 1,
                                            color: colorHairline,
                                          ),
                                        ),
                                      ],
                                    ),
                                  );
                                }).toList(),
                              ),
                              // Clash zones (hatched overlay)
                              ...clashZones.map((z) {
                                final left = xPct(z.start) * w + 32;
                                final width = (xPct(z.end) - xPct(z.start)) * w;
                                return Positioned(
                                  left: left,
                                  top: 0,
                                  bottom: 0,
                                  width: width,
                                  child: CustomPaint(painter: _HatchPainter()),
                                );
                              }),
                            ],
                          ),
                        ),
                        // Legend
                        const SizedBox(height: 10),
                        Row(
                          children: [
                            _LegendItem(
                              color: colorSurface2,
                              borderColor: colorFg3,
                              label: 'SCHEDULED',
                            ),
                            const SizedBox(width: 12),
                            _LegendItem(
                              color: colorAccentWash,
                              borderColor: colorAccent,
                              label: '★ STARRED',
                            ),
                            const SizedBox(width: 12),
                            _LegendItem(
                              isHatch: true,
                              label: 'CLASH',
                              labelColor: colorWarn,
                            ),
                          ],
                        ),
                      ],
                    );
                  },
                ),
              ],
            ),
          ),
        ),
        // Clash list header
        Padding(
          padding: const EdgeInsets.fromLTRB(18, 14, 18, 8),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              const Text(
                '! RESOLVE',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.08 * 11,
                  color: colorWarn,
                  height: 1,
                ),
              ),
              Text(
                '${clashPairs.length} conflict${clashPairs.length == 1 ? '' : 's'}',
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  color: colorFg3,
                  height: 1,
                ),
              ),
            ],
          ),
        ),
        // Clash cards
        ...clashPairs.map(
          (pair) => Padding(
            padding: const EdgeInsets.fromLTRB(18, 0, 18, 12),
            child: _ClashCard(
              setA: pair[0],
              setB: pair[1],
              stageA: stageById[pair[0].stage]!,
              stageB: stageById[pair[1].stage]!,
            ),
          ),
        ),
        const SizedBox(height: 80),
      ],
    );
  }
}

class _LegendItem extends StatelessWidget {
  final Color? color;
  final Color? borderColor;
  final bool isHatch;
  final String label;
  final Color labelColor;

  const _LegendItem({
    this.color,
    this.borderColor,
    this.isHatch = false,
    required this.label,
    this.labelColor = colorFg3,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (isHatch)
          SizedBox(
            width: 14,
            height: 6,
            child: CustomPaint(painter: _HatchPainter()),
          )
        else
          Container(
            width: 14,
            height: 6,
            color: color,
            foregroundDecoration: BoxDecoration(
              border: Border(
                left: BorderSide(color: borderColor ?? colorFg3, width: 2),
              ),
            ),
          ),
        const SizedBox(width: 6),
        Text(
          label,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 9,
            letterSpacing: 0.08 * 9,
            color: labelColor,
            height: 1,
          ),
        ),
      ],
    );
  }
}

class _HatchPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = colorWarn.withValues(alpha: 0.18)
      ..strokeWidth = 4;
    // Left/right borders
    final borderPaint = Paint()
      ..color = colorWarn
      ..strokeWidth = 1.5;
    canvas.drawLine(Offset(0, 0), Offset(0, size.height), borderPaint);
    canvas.drawLine(
      Offset(size.width, 0),
      Offset(size.width, size.height),
      borderPaint,
    );

    // Hatching
    for (double x = -size.height; x < size.width + size.height; x += 8) {
      canvas.drawLine(
        Offset(x, 0),
        Offset(x + size.height, size.height),
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(_) => false;
}

class _ClashCard extends StatefulWidget {
  final FestSet setA;
  final FestSet setB;
  final Stage stageA;
  final Stage stageB;

  const _ClashCard({
    required this.setA,
    required this.setB,
    required this.stageA,
    required this.stageB,
  });

  @override
  State<_ClashCard> createState() => _ClashCardState();
}

class _ClashCardState extends State<_ClashCard> {
  String _chosen = 'a';

  @override
  Widget build(BuildContext context) {
    final a = widget.setA, b = widget.setB;
    final sA = widget.stageA, sB = widget.stageB;
    final overlapStart = [a.t, b.t].reduce((v, e) => v > e ? v : e);
    final overlapEnd = [
      a.t + a.dur,
      b.t + b.dur,
    ].reduce((v, e) => v < e ? v : e);
    final overlapMin = overlapEnd - overlapStart;

    return Container(
      decoration: BoxDecoration(
        color: colorWarn.withValues(alpha: 0.04),
        border: Border.all(color: colorWarn, width: 1.5),
      ),
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            'OVERLAP ${fmtTime(overlapStart)} → ${fmtTime(overlapEnd)} · $overlapMin MIN',
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              letterSpacing: 0.08 * 10,
              color: colorWarn,
              height: 1,
            ),
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: _ClashOption(
                  set: a,
                  stage: sA,
                  chosen: _chosen == 'a',
                  onTap: () => setState(() => _chosen = 'a'),
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 12),
                child: const Text(
                  'VS',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    color: colorWarn,
                    height: 1,
                  ),
                ),
              ),
              Expanded(
                child: _ClashOption(
                  set: b,
                  stage: sB,
                  chosen: _chosen == 'b',
                  onTap: () => setState(() => _chosen = 'b'),
                ),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 6,
            children: [
              MonoChip(label: 'SPLIT — 30M EACH', onTap: () {}),
              MonoChip(
                label:
                    'UNSTAR ${_chosen == 'a' ? b.artist.split(' ').first : a.artist.split(' ').first}',
                onTap: () {},
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _ClashOption extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final bool chosen;
  final VoidCallback onTap;

  const _ClashOption({
    required this.set,
    required this.stage,
    required this.chosen,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        decoration: BoxDecoration(
          color: chosen ? colorSurface2 : colorSurface1,
          border: Border(left: BorderSide(color: Color(stage.color), width: 3)),
          boxShadow: chosen
              ? [
                  BoxShadow(
                    color: colorFg3.withValues(alpha: 0.3),
                    blurRadius: 0,
                    spreadRadius: 1,
                  ),
                ]
              : null,
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              '★ ${set.artist}',
              style: const TextStyle(
                fontFamily: 'Helvetica',
                fontWeight: FontWeight.w700,
                fontSize: 14,
                letterSpacing: -0.02 * 14,
                color: colorFg,
                height: 1,
              ),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 2),
            Text(
              '${stage.name} · ${fmtTime(set.t)} → ${fmtTime(set.t + set.dur)}',
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
    );
  }
}
