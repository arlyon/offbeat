// OFFBEAT StageTabsView — V3 Stage tabs
// Horizontal scrolling stage tabs with color swatch + name + count + live flag
// Day pill row above
// Stage hero card: 4px accent stripe left, "// STAGE PROFILE" super, big name, meta
// Now-on-stage callout (if live): dotted accent border, accent-wash bg
// Lineup with BigCard components

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/star_button.dart';
import '../../widgets/live_dot.dart';
import '../../widgets/chip.dart';

class StageTabsView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final void Function(String setId)? onStar;

  const StageTabsView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    this.onStar,
  });

  @override
  State<StageTabsView> createState() => _StageTabsViewState();
}

class _StageTabsViewState extends State<StageTabsView> {
  late String _stageId;
  late String _day;

  @override
  void initState() {
    super.initState();
    _day = widget.days.first.id;
    _stageId = widget.stages.first.id;
  }

  Stage get _currentStage => widget.stages.firstWhere((s) => s.id == _stageId);

  List<FestSet> get _dayStageSets {
    final s = widget.sets.where((s) => s.day == _day && s.stage == _stageId).toList();
    s.sort((a, b) => a.t.compareTo(b.t));
    return s;
  }

  Map<String, List<FestSet>> get _setsByStage {
    final m = <String, List<FestSet>>{};
    for (final s in widget.stages) {
      m[s.id] = [];
    }
    for (final s in widget.sets.where((s) => s.day == _day)) {
      m[s.stage]?.add(s);
    }
    return m;
  }

  FestSet? get _liveSet => _dayStageSets.cast<FestSet?>().firstWhere(
    (s) => s!.live,
    orElse: () => null,
  );

  @override
  Widget build(BuildContext context) {
    final stage = _currentStage;
    final sets = _dayStageSets;
    final live = _liveSet;
    final setsByStage = _setsByStage;

    return Column(
      children: [
        // Day pill row (hidden for single-day festivals)
        if (widget.days.length > 1)
          _DayPillRow(
            days: widget.days,
            activeDay: _day,
            onDayChanged: (d) => setState(() => _day = d),
          ),
        // Stage tabs
        _StageTabs(
          stages: widget.stages,
          activeStageId: _stageId,
          day: _day,
          sets: widget.sets,
          onStageChanged: (id) => setState(() => _stageId = id),
          setsByStage: setsByStage,
        ),
        // Scrollable content
        Expanded(
          child: ListView(
            padding: EdgeInsets.zero,
            children: [
              // Stage hero card
              _StageHero(stage: stage, sets: sets),
              // Now-on-stage callout
              if (live != null) _NowCallout(set: live, stage: stage),
              // Lineup eyebrow
              Padding(
                padding: const EdgeInsets.fromLTRB(18, 20, 18, 8),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    const Text(
                      '// LINEUP',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 11,
                        color: colorFg3,
                        letterSpacing: 0.08 * 11,
                        height: 1,
                      ),
                    ),
                    Text(
                      '${widget.days.firstWhere((d) => d.id == _day).label} '
                      '${widget.days.firstWhere((d) => d.id == _day).dayNum}',
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        color: colorFg4,
                        letterSpacing: 0.08 * 10,
                        height: 1,
                      ),
                    ),
                  ],
                ),
              ),
              // BigCard lineup
              ...sets.map(
                (s) => _BigCard(
                  set: s,
                  stage: stage,
                  onStar: () => widget.onStar?.call(s.id),
                ),
              ),
              const SizedBox(height: 80),
            ],
          ),
        ),
      ],
    );
  }
}

class _DayPillRow extends StatelessWidget {
  final List<Day> days;
  final String activeDay;
  final ValueChanged<String> onDayChanged;

  const _DayPillRow({
    required this.days,
    required this.activeDay,
    required this.onDayChanged,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        child: Row(
          children: [
            const Text(
              '// DAY',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.1 * 9,
                color: colorFg3,
                height: 1,
              ),
            ),
            const SizedBox(width: 8),
            ...days.map(
              (d) => Padding(
                padding: const EdgeInsets.only(left: 8),
                child: MonoChip(
                  label: '${d.label} ${d.dayNum}',
                  active: d.id == activeDay,
                  onTap: () => onDayChanged(d.id),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _StageTabs extends StatelessWidget {
  final List<Stage> stages;
  final String activeStageId;
  final String day;
  final List<FestSet> sets;
  final ValueChanged<String> onStageChanged;
  final Map<String, List<FestSet>> setsByStage;

  const _StageTabs({
    required this.stages,
    required this.activeStageId,
    required this.day,
    required this.sets,
    required this.onStageChanged,
    required this.setsByStage,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SingleChildScrollView(
        scrollDirection: Axis.horizontal,
        child: Row(
          children: stages.map((s) {
            final isActive = s.id == activeStageId;
            final stageColor = Color(s.color);
            final ct = setsByStage[s.id]?.length ?? 0;
            final liveOn = sets.any(
              (x) => x.stage == s.id && x.day == day && x.live,
            );

            return GestureDetector(
              onTap: () => onStageChanged(s.id),
              child: Stack(
                children: [
                  // Active bottom accent line
                  if (isActive)
                    Positioned(
                      bottom: 0,
                      left: 0,
                      right: 0,
                      child: Container(height: 2, color: stageColor),
                    ),
                  // Right dotted border
                  if (stages.last.id != s.id)
                    const Positioned(
                      right: 0,
                      top: 0,
                      bottom: 0,
                      width: 1.5,
                      child: VerticalDottedRule(),
                    ),
                  Container(
                    color: isActive ? colorSurface1 : Colors.transparent,
                    padding: const EdgeInsets.fromLTRB(14, 12, 14, 10),
                    constraints: const BoxConstraints(minWidth: 96),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        // Color swatch
                        Container(width: 18, height: 3, color: stageColor),
                        const SizedBox(height: 8),
                        Text(
                          s.name,
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 0.08 * 11,
                            color: isActive ? colorFg : colorFg2,
                            height: 1,
                          ),
                        ),
                        const SizedBox(height: 2),
                        Text(
                          '$ct sets',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            letterSpacing: 0.08 * 9,
                            color: colorFg4,
                            height: 1,
                          ),
                        ),
                        if (liveOn) ...[
                          const SizedBox(height: 4),
                          const LiveDot(size: 6),
                        ],
                      ],
                    ),
                  ),
                ],
              ),
            );
          }).toList(),
        ),
      ),
    );
  }
}

class _StageHero extends StatelessWidget {
  final Stage stage;
  final List<FestSet> sets;

  const _StageHero({required this.stage, required this.sets});

  @override
  Widget build(BuildContext context) {
    final stageColor = Color(stage.color);
    final totalDur = sets.isEmpty ? 0 : sets.fold(0, (acc, s) => acc + s.dur);
    final firstT = sets.isEmpty ? 0 : sets.first.t;
    final lastT = sets.isEmpty ? 0 : sets.last.t + sets.last.dur;

    return DottedBorder.bottom(
      child: Stack(
        children: [
          // Left accent stripe
          Positioned(
            left: 0,
            top: 0,
            bottom: 0,
            child: Container(width: 4, color: stageColor),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 18, 18, 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const Text(
                  '// STAGE PROFILE',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.1 * 10,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
                const SizedBox(height: 6),
                Text(
                  stage.name,
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontWeight: FontWeight.w700,
                    fontSize: 36,
                    letterSpacing: -0.03 * 36,
                    height: 1,
                    color: colorFg,
                  ),
                ),
                const SizedBox(height: 6),
                Wrap(
                  spacing: 8,
                  children: [
                    _MetaItem('${sets.length} SETS'),
                    const _MetaSep(),
                    _MetaItem('${(totalDur / 60).round()}H PROGRAMMING'),
                    const _MetaSep(),
                    if (sets.isNotEmpty)
                      _MetaItem('${fmtTime(firstT)} → ${fmtTime(lastT)}'),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _MetaItem extends StatelessWidget {
  final String text;
  const _MetaItem(this.text);

  @override
  Widget build(BuildContext context) {
    return Text(
      text,
      style: const TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: 11,
        color: colorFg3,
        letterSpacing: 0.08 * 11,
        height: 1,
      ),
    );
  }
}

class _MetaSep extends StatelessWidget {
  const _MetaSep();

  @override
  Widget build(BuildContext context) {
    return const Text('|', style: TextStyle(color: colorFg4, fontSize: 11));
  }
}

class _NowCallout extends StatelessWidget {
  final FestSet set;
  final Stage stage;

  const _NowCallout({required this.set, required this.stage});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 14, 18, 0),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        decoration: BoxDecoration(
          color: colorAccentWash,
          border: Border.all(color: colorAccent, width: 1.5),
        ),
        child: Row(
          children: [
            const LiveDot(size: 7),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    set.artist,
                    style: const TextStyle(
                      fontFamily: 'Helvetica',
                      fontWeight: FontWeight.w700,
                      fontSize: 18,
                      letterSpacing: -0.02 * 18,
                      color: colorFg,
                      height: 1,
                    ),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    '${fmtTime(set.t)} → ${fmtTime(set.t + set.dur)} · ${set.genre}',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      letterSpacing: 0.08 * 10,
                      color: colorFg2,
                      height: 1,
                    ),
                  ),
                ],
              ),
            ),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
              color: colorAccent,
              child: const Text(
                'LIVE',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.1 * 10,
                  color: colorAccentInk,
                  height: 1,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _BigCard extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final VoidCallback onStar;

  const _BigCard({
    required this.set,
    required this.stage,
    required this.onStar,
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
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 14),
            child: Row(
              children: [
                // Time (56px)
                SizedBox(
                  width: 56,
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
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
                      Text(
                        '→ ${fmtTime(set.t + set.dur)}',
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          color: colorFg4,
                          height: 1,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 14),
                // Name + meta
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        set.artist,
                        style: const TextStyle(
                          fontFamily: 'Helvetica',
                          fontWeight: FontWeight.w700,
                          fontSize: 18,
                          letterSpacing: -0.02 * 18,
                          height: 1.1,
                          color: colorFg,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Wrap(
                        spacing: 6,
                        children: [
                          if (set.live) const LiveDot(size: 6),
                          Text(
                            '${set.dur} MIN',
                            style: const TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 10,
                              color: colorFg3,
                              letterSpacing: 0.08 * 10,
                            ),
                          ),
                          const Text('|', style: TextStyle(color: colorFg4)),
                          Text(
                            set.genre,
                            style: const TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 10,
                              color: colorFg3,
                              letterSpacing: 0.08 * 10,
                            ),
                          ),
                          if (set.clashes.isNotEmpty) ...[
                            const Text('|', style: TextStyle(color: colorFg4)),
                            const Text(
                              '! CLASH',
                              style: TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 10,
                                color: colorWarn,
                              ),
                            ),
                          ],
                        ],
                      ),
                    ],
                  ),
                ),
                // Star
                StarButton(starred: set.starred, onToggle: onStar, size: 22),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
