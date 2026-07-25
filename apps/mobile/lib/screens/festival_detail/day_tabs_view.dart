// OFFBEAT DayTabsView — V2 Day tabs + hour-grouped set list
// Ticket-stub day picker: grid buttons with month/dow/date-number/set-count
// Active day: accent indicators (colored number, 2px bottom line)
// Hour-grouped set list with sticky hour headers
// SetRow: grid [56px time | 4px color bar | name + sub | star]

import 'package:flutter/material.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/co_liker_pins.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/star_button.dart';
import '../../widgets/live_dot.dart';

class DayTabsView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final String festivalWhere;
  final void Function(String setId)? onStar;

  const DayTabsView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    required this.festivalWhere,
    this.onStar,
  });

  @override
  State<DayTabsView> createState() => _DayTabsViewState();
}

class _DayTabsViewState extends State<DayTabsView> {
  late String _day;

  @override
  void initState() {
    super.initState();
    _day = widget.days.first.id;
  }

  Map<String, Stage> get _stageById {
    return {for (final s in widget.stages) s.id: s};
  }

  List<FestSet> get _daySets {
    final s = widget.sets.where((s) => s.day == _day).toList();
    s.sort((a, b) => a.t.compareTo(b.t));
    return s;
  }

  // Group by hour bucket
  Map<int, List<FestSet>> get _grouped {
    final g = <int, List<FestSet>>{};
    for (final s in _daySets) {
      final hr = s.t ~/ 60;
      g.putIfAbsent(hr, () => []).add(s);
    }
    return Map.fromEntries(
      g.entries.toList()..sort((a, b) => a.key.compareTo(b.key)),
    );
  }

  @override
  Widget build(BuildContext context) {
    final stageById = _stageById;
    final grouped = _grouped;
    final daySets = _daySets;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Page title
        Padding(
          padding: const EdgeInsets.fromLTRB(18, 14, 18, 4),
          child: const Text(
            'Set times.',
            style: TextStyle(
              fontFamily: 'Helvetica',
              fontWeight: FontWeight.w700,
              fontSize: 32,
              letterSpacing: -0.02 * 32,
              height: 1,
              color: colorFg,
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(18, 0, 18, 16),
          child: Text(
            '${widget.festivalWhere}  |  ${daySets.length} sets',
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              color: colorFg3,
              letterSpacing: 0.08 * 11,
              height: 1,
            ),
          ),
        ),
        // Day ticket-stub strip (hidden for single-day festivals)
        if (widget.days.length > 1)
          _DayTabStrip(
            days: widget.days,
            sets: widget.sets,
            activeDay: _day,
            onDayChanged: (d) => setState(() => _day = d),
          ),
        // Set list
        Expanded(
          child: ListView(
            padding: EdgeInsets.zero,
            children: [
              for (final entry in grouped.entries) ...[
                _HourHeader(hour: entry.key, sets: entry.value),
                ...entry.value.map(
                  (s) => SetRow(
                    set: s,
                    stage: stageById[s.stage]!,
                    onStar: widget.onStar,
                  ),
                ),
              ],
              const SizedBox(height: 80),
            ],
          ),
        ),
      ],
    );
  }
}

class _DayTabStrip extends StatelessWidget {
  final List<Day> days;
  final List<FestSet> sets;
  final String activeDay;
  final ValueChanged<String> onDayChanged;

  const _DayTabStrip({
    required this.days,
    required this.sets,
    required this.activeDay,
    required this.onDayChanged,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Row(
        children: days.map((d) {
          final isActive = d.id == activeDay;
          final ct = sets.where((s) => s.day == d.id).length;
          return Expanded(
            child: GestureDetector(
              onTap: () => onDayChanged(d.id),
              child: Stack(
                children: [
                  // Active bottom line
                  if (isActive)
                    Positioned(
                      bottom: 0,
                      left: 0,
                      right: 0,
                      child: Container(height: 2, color: colorAccent),
                    ),
                  // Right border (dotted)
                  if (days.last.id != d.id)
                    const Positioned(
                      right: 0,
                      top: 0,
                      bottom: 0,
                      width: 1.5,
                      child: VerticalDottedRule(),
                    ),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(12, 14, 12, 12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '// ${d.month}',
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            letterSpacing: 0.1 * 9,
                            color: isActive ? colorAccent : colorFg4,
                            height: 1,
                          ),
                        ),
                        const SizedBox(height: 2),
                        Text(
                          d.label,
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            fontWeight: FontWeight.w700,
                            letterSpacing: 0.12 * 11,
                            color: isActive ? colorFg : colorFg2,
                            height: 1,
                          ),
                        ),
                        const SizedBox(height: 6),
                        Text(
                          d.dayNum,
                          style: TextStyle(
                            fontFamily: 'Helvetica',
                            fontWeight: FontWeight.w700,
                            fontSize: 30,
                            letterSpacing: -0.03 * 30,
                            height: 1,
                            color: isActive ? colorAccent : colorFg2,
                          ),
                        ),
                        const SizedBox(height: 6),
                        Text(
                          '$ct sets',
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            letterSpacing: 0.08 * 9,
                            color: isActive ? colorFg2 : colorFg4,
                            height: 1,
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          );
        }).toList(),
      ),
    );
  }
}

class _HourHeader extends StatelessWidget {
  final int hour;
  final List<FestSet> sets;

  const _HourHeader({required this.hour, required this.sets});

  @override
  Widget build(BuildContext context) {
    final hr = hour % 24;
    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 14, 18, 6),
        child: Row(
          children: [
            Text(
              '${hr.toString().padLeft(2, '0')}:00',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 14,
                fontWeight: FontWeight.w500,
                color: colorFg,
                height: 1,
              ),
            ),
            const SizedBox(width: 10),
            Text(
              '→ ${((hr + 1) % 24).toString().padLeft(2, '0')}:00',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: 0.1 * 9,
                color: colorFg4,
                height: 1,
              ),
            ),
            const Spacer(),
            Text(
              '${sets.length} sets',
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
      ),
    );
  }
}

class SetRow extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final void Function(String setId)? onStar;

  const SetRow({
    super.key,
    required this.set,
    required this.stage,
    this.onStar,
  });

  @override
  Widget build(BuildContext context) {
    final stageColor = Color(stage.color);
    final isLive = set.live;
    final hasClash = set.clashes.isNotEmpty;

    return DottedBorder.bottom(
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: () {},
          splashColor: Colors.transparent,
          highlightColor: colorSurface1,
          child: Container(
            decoration: isLive
                ? const BoxDecoration(
                    gradient: LinearGradient(
                      colors: [colorAccentWash, Colors.transparent],
                      stops: [0.0, 0.7],
                    ),
                  )
                : null,
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
            child: Row(
              children: [
                // Time column (56px)
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
                          fontSize: 13,
                          fontWeight: FontWeight.w500,
                          color: colorFg2,
                          height: 1.2,
                        ),
                      ),
                      Text(
                        '→ ${fmtTime(set.t + set.dur)}',
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          color: colorFg4,
                          height: 1.2,
                        ),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                // Color bar (4px)
                Container(width: 4, height: 40, color: stageColor),
                const SizedBox(width: 12),
                // Name + sub
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          if (isLive)
                            const Padding(
                              padding: EdgeInsets.only(right: 8),
                              child: LiveDot(size: 7),
                            ),
                          Flexible(
                            child: Text(
                              set.artist,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: const TextStyle(
                                fontFamily: 'Helvetica',
                                fontWeight: FontWeight.w700,
                                fontSize: 16,
                                letterSpacing: -0.01 * 16,
                                height: 1.15,
                                color: colorFg,
                              ),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 3),
                      Text(
                        hasClash
                            ? '! CLASH × ${set.clashes.length} STARRED'
                            : '${stage.name} | ${set.dur} MIN | ${set.genre}',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          color: hasClash ? colorWarn : colorFg3,
                          letterSpacing: 0.08 * 10,
                          height: 1,
                        ),
                      ),
                      if (set.supporters.isNotEmpty)
                        CoLikerPins(
                          artist: set.artist,
                          supporters: set.supporters,
                        ),
                    ],
                  ),
                ),
                // Star
                StarButton(
                  starred: set.starred,
                  onToggle: () => onStar?.call(set.id),
                  size: 18,
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
