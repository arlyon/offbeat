// OFFBEAT GanttView — V1 Gantt-Scroll (THE signature view)
// Time on X axis (18:00-02:00), 6 stage rows on Y
// Content width: 480min × 3px/min = 1440px
// Stage labels (46px) sticky on left
// CRITICAL INTERACTION: User scrolls sentinel vertically → maps to horizontal pan
// Time axis: hour ticks + half-hour marks, centered-time badge top-right
// Set blocks: absolute positioned, 3px colored left border per stage
// Starred sets: accent-wash bg. Live sets: accent border glow
// NOW line: 2px accent vertical, 8px dot at top
// Bottom HUD: scrubber bar + "scroll ↓" hint with bob animation

import 'package:flutter/material.dart';
import '../../data/mock_data.dart';
import '../../theme/tokens.dart';
import '../../widgets/live_dot.dart';
import '../../widgets/dotted_border.dart';

class GanttView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;

  const GanttView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
  });

  @override
  State<GanttView> createState() => _GanttViewState();
}

class _GanttViewState extends State<GanttView> {
  String _day = 'fri';
  double _progress = 0.0;
  double _viewportInnerW = 0.0;

  final ScrollController _scrollController = ScrollController();
  static const double _sentinelHeight = 1500.0;

  double get _maxTx => (ganttContentW - _viewportInnerW).clamp(0.0, double.infinity);
  double get _tx => _progress * _maxTx;

  // What time is centered?
  String get _centerTimeStr {
    final centerMin = ganttStartMin + (_tx + _viewportInnerW / 2) / ganttPxPerMin;
    final h = (centerMin ~/ 60) % 24;
    final m = centerMin ~/ 1 % 60;
    return '${h.toString().padLeft(2, '0')}:${m.toString().padLeft(2, '0')}';
  }

  // Now line x position (relative to inner content before translate)
  double get _nowX => (kNowT - ganttStartMin) * ganttPxPerMin;

  List<int> get _axisHours {
    final arr = <int>[];
    for (int m = ganttStartMin; m <= ganttEndMin; m += 60) arr.add(m);
    return arr;
  }

  @override
  void initState() {
    super.initState();
    _scrollController.addListener(_onScroll);
  }

  @override
  void dispose() {
    _scrollController.removeListener(_onScroll);
    _scrollController.dispose();
    super.dispose();
  }

  void _onScroll() {
    final el = _scrollController;
    final maxScroll = el.position.maxScrollExtent;
    if (maxScroll <= 0) {
      setState(() => _progress = 0);
      return;
    }
    final p = (el.offset / maxScroll).clamp(0.0, 1.0);
    setState(() => _progress = p);
  }

  void _centerOnNow() {
    if (_viewportInnerW <= 0) return;
    final nowX = (kNowT - ganttStartMin) * ganttPxPerMin;
    final targetTx = (nowX - _viewportInnerW / 2).clamp(0.0, _maxTx);
    final targetP = _maxTx > 0 ? targetTx / _maxTx : 0.0;
    final maxScroll = _scrollController.position.maxScrollExtent;
    _scrollController.jumpTo(targetP * maxScroll);
  }

  @override
  Widget build(BuildContext context) {
    final daySets = widget.sets.where((s) => s.day == _day).toList();

    return Column(
      children: [
        // Meta strip: now time + day picker
        _MetaStrip(
          day: _day,
          days: widget.days,
          onDayChanged: (d) => setState(() {
            _day = d;
            // Reset scroll when day changes
            WidgetsBinding.instance.addPostFrameCallback((_) => _centerOnNow());
          }),
        ),
        // Gantt viewport
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final vw = constraints.maxWidth - ganttStageLabelW;
              if (_viewportInnerW != vw) {
                WidgetsBinding.instance.addPostFrameCallback((_) {
                  setState(() => _viewportInnerW = vw);
                  _centerOnNow();
                });
              }

              return Stack(
                children: [
                  // Gantt content rendered first (below the sentinel)
                  Positioned.fill(
                    child: IgnorePointer(
                      child: _GanttContent(
                        tx: _tx,
                        progress: _progress,
                        viewportInnerW: _viewportInnerW,
                        daySets: daySets,
                        stages: widget.stages,
                        axisHours: _axisHours,
                        nowX: _nowX,
                        centerTimeStr: _centerTimeStr,
                        currentDay: _day,
                      ),
                    ),
                  ),
                  // Scroll sentinel on top (captures all scroll events)
                  Positioned.fill(
                    child: SingleChildScrollView(
                      controller: _scrollController,
                      child: const SizedBox(
                        width: 1,
                        height: _sentinelHeight,
                      ),
                    ),
                  ),
                  // Bottom HUD — always on top
                  Positioned(
                    left: 0,
                    right: 0,
                    bottom: 0,
                    child: IgnorePointer(
                      child: _GanttHUD(
                        progress: _progress,
                        tx: _tx,
                        viewportInnerW: _viewportInnerW,
                      ),
                    ),
                  ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}

class _MetaStrip extends StatelessWidget {
  final String day;
  final List<Day> days;
  final ValueChanged<String> onDayChanged;

  const _MetaStrip({
    required this.day,
    required this.days,
    required this.onDayChanged,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
        child: Row(
          children: [
            Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  children: [
                    const Text(
                      '// NOW',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 9,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.1 * 9,
                        color: colorFg3,
                        height: 1,
                      ),
                    ),
                    const SizedBox(width: 6),
                    const LiveDot(size: 6),
                  ],
                ),
                const SizedBox(height: 2),
                Text(
                  fmtTime(kNowT),
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 12,
                    color: colorFg,
                    height: 1,
                  ),
                ),
              ],
            ),
            const Spacer(),
            // Day pill buttons
            Row(
              mainAxisSize: MainAxisSize.min,
              children: days.map((d) {
                final isActive = d.id == day;
                return Padding(
                  padding: const EdgeInsets.only(left: 4),
                  child: GestureDetector(
                    onTap: () => onDayChanged(d.id),
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                      decoration: BoxDecoration(
                        color: isActive ? colorFg : Colors.transparent,
                        border: Border.all(
                          color: isActive ? colorFg : colorDotted,
                          width: 1.5,
                        ),
                      ),
                      child: Text(
                        '${d.label} ${d.num}',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.08 * 10,
                          color: isActive ? colorBg : colorFg2,
                          height: 1,
                        ),
                      ),
                    ),
                  ),
                );
              }).toList(),
            ),
          ],
        ),
      ),
    );
  }
}

class _GanttContent extends StatelessWidget {
  final double tx;
  final double progress;
  final double viewportInnerW;
  final List<FestSet> daySets;
  final List<Stage> stages;
  final List<int> axisHours;
  final double nowX;
  final String centerTimeStr;
  final String currentDay;

  const _GanttContent({
    required this.tx,
    required this.progress,
    required this.viewportInnerW,
    required this.daySets,
    required this.stages,
    required this.axisHours,
    required this.nowX,
    required this.centerTimeStr,
    required this.currentDay,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        // Time axis
        _TimeAxis(
          tx: tx,
          axisHours: axisHours,
          centerTimeStr: centerTimeStr,
        ),
        // Stage rows
        Expanded(
          child: _StageRows(
            tx: tx,
            daySets: daySets,
            stages: stages,
            nowX: nowX,
            currentDay: currentDay,
          ),
        ),
      ],
    );
  }
}

class _TimeAxis extends StatelessWidget {
  final double tx;
  final List<int> axisHours;
  final String centerTimeStr;

  const _TimeAxis({
    required this.tx,
    required this.axisHours,
    required this.centerTimeStr,
  });

  @override
  Widget build(BuildContext context) {
    const tickW = 60.0 * ganttPxPerMin; // 180px per hour

    return DottedBorder.bottom(
      child: SizedBox(
        height: 36,
        child: Stack(
          clipBehavior: Clip.hardEdge,
          children: [
            // Hour ticks
            Positioned(
              left: 0,
              top: 0,
              bottom: 0,
              child: Transform.translate(
                offset: Offset(ganttStageLabelW - tx, 0),
                child: Row(
                  children: axisHours.map((m) {
                    return SizedBox(
                      width: tickW,
                      child: Stack(
                        children: [
                          // Hour label
                          Padding(
                            padding: const EdgeInsets.only(left: 10, top: 8),
                            child: Text(
                              fmtTime(m),
                              style: const TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 11,
                                color: colorFg2,
                                height: 1,
                              ),
                            ),
                          ),
                          // Right border (hour tick)
                          Positioned(
                            right: 0,
                            top: 0,
                            bottom: 0,
                            child: Container(
                              width: 1,
                              color: colorDotted,
                            ),
                          ),
                          // Half-hour mark
                          Positioned(
                            left: tickW / 2,
                            top: 0,
                            bottom: 0,
                            child: Container(
                              width: 1,
                              color: colorHairline,
                            ),
                          ),
                        ],
                      ),
                    );
                  }).toList(),
                ),
              ),
            ),
            // Centered time badge
            Positioned(
              right: 8,
              top: 8,
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                decoration: BoxDecoration(
                  color: colorBg,
                  border: Border.all(color: colorAccent, width: 1.5),
                ),
                child: Text(
                  centerTimeStr,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    color: colorAccent,
                    letterSpacing: -0.02 * 11,
                    height: 1,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _StageRows extends StatelessWidget {
  final double tx;
  final List<FestSet> daySets;
  final List<Stage> stages;
  final double nowX;
  final String currentDay;

  const _StageRows({
    required this.tx,
    required this.daySets,
    required this.stages,
    required this.nowX,
    required this.currentDay,
  });

  @override
  Widget build(BuildContext context) {
    return Stack(
      clipBehavior: Clip.hardEdge,
      children: [
        // Stage rows container (translates horizontally)
        Column(
          children: stages.asMap().entries.map((entry) {
            final i = entry.key;
            final stage = entry.value;
            final stageSets = daySets.where((s) => s.stage == stage.id).toList();
            return Expanded(
              child: _SingleStageRow(
                stage: stage,
                sets: stageSets,
                tx: tx,
                isLast: i == stages.length - 1,
              ),
            );
          }).toList(),
        ),
        // NOW line
        if (currentDay == kNowDay)
          Positioned(
            left: ganttStageLabelW + nowX - tx,
            top: 0,
            bottom: 0,
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                Container(width: 2, color: colorAccent),
                Positioned(
                  top: -4,
                  left: -3,
                  child: Container(
                    width: 8,
                    height: 8,
                    decoration: const BoxDecoration(
                      shape: BoxShape.circle,
                      color: colorAccent,
                    ),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _SingleStageRow extends StatelessWidget {
  final Stage stage;
  final List<FestSet> sets;
  final double tx;
  final bool isLast;

  const _SingleStageRow({
    required this.stage,
    required this.sets,
    required this.tx,
    required this.isLast,
  });

  @override
  Widget build(BuildContext context) {
    final stageColor = Color(stage.color);
    return Stack(
      clipBehavior: Clip.hardEdge,
      children: [
        // Bottom dotted border
        if (!isLast)
          Positioned(
            bottom: 0,
            left: 0,
            right: 0,
            child: const DottedRule(),
          ),
        // Stage label (sticky left)
        Positioned(
          left: 0,
          top: 0,
          bottom: 0,
          width: ganttStageLabelW,
          child: Container(
            decoration: BoxDecoration(
              gradient: LinearGradient(
                begin: Alignment.centerLeft,
                end: Alignment.centerRight,
                colors: [colorBg, colorBg.withOpacity(0)],
                stops: const [0.6, 1.0],
              ),
            ),
            padding: const EdgeInsets.only(left: 10),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  stage.short,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.1 * 9,
                    color: colorFg3,
                    height: 1,
                  ),
                ),
                const SizedBox(height: 4),
                Container(
                  width: 14,
                  height: 3,
                  color: stageColor,
                ),
              ],
            ),
          ),
        ),
        // Set blocks
        ...sets.map((s) {
          final left = (s.t - ganttStartMin) * ganttPxPerMin + ganttStageLabelW - tx;
          final width = s.dur * ganttPxPerMin;
          if (left + width < 0 || left > 2000) return const SizedBox.shrink();

          return Positioned(
            left: left,
            top: 6,
            bottom: 6,
            width: width,
            child: _SetBlock(set: s, stageColor: stageColor),
          );
        }),
      ],
    );
  }
}

class _SetBlock extends StatelessWidget {
  final FestSet set;
  final Color stageColor;

  const _SetBlock({required this.set, required this.stageColor});

  @override
  Widget build(BuildContext context) {
    Color bg = colorSurface1;
    BoxDecoration decoration = BoxDecoration(
      color: bg,
      border: Border(left: BorderSide(color: stageColor, width: 3)),
    );

    if (set.live) {
      decoration = BoxDecoration(
        color: colorAccentWash,
        border: Border(
          left: BorderSide(color: stageColor, width: 3),
          top: const BorderSide(color: colorAccent, width: 1),
          right: const BorderSide(color: colorAccent, width: 1),
          bottom: const BorderSide(color: colorAccent, width: 1),
        ),
      );
    } else if (set.starred) {
      decoration = BoxDecoration(
        color: colorAccentWash,
        border: Border(left: BorderSide(color: stageColor, width: 3)),
      );
    }

    return Container(
      decoration: decoration,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 5),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (set.starred)
                const Text('★ ', style: TextStyle(color: colorAccent, fontSize: 10, height: 1)),
              if (set.live)
                const Padding(
                  padding: EdgeInsets.only(right: 4),
                  child: LiveDot(size: 6),
                ),
              Flexible(
                child: Text(
                  set.artist,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontWeight: FontWeight.w700,
                    fontSize: 12,
                    letterSpacing: -0.01 * 12,
                    color: colorFg,
                    height: 1.1,
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 2),
          Text(
            '${fmtTime(set.t)} → ${fmtTime(set.t + set.dur)}',
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 9,
              color: colorFg3,
              height: 1,
            ),
          ),
        ],
      ),
    );
  }
}

class _GanttHUD extends StatefulWidget {
  final double progress;
  final double tx;
  final double viewportInnerW;

  const _GanttHUD({
    required this.progress,
    required this.tx,
    required this.viewportInnerW,
  });

  @override
  State<_GanttHUD> createState() => _GanttHUDState();
}

class _GanttHUDState extends State<_GanttHUD> with SingleTickerProviderStateMixin {
  late AnimationController _bobController;
  late Animation<double> _bobAnim;

  @override
  void initState() {
    super.initState();
    _bobController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1600),
    )..repeat(reverse: true);
    _bobAnim = Tween<double>(begin: 0.0, end: 3.0).animate(
      CurvedAnimation(parent: _bobController, curve: Curves.easeInOut),
    );
  }

  @override
  void dispose() {
    _bobController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final startMin = (ganttStartMin + widget.tx / ganttPxPerMin).round();
    final endMin = (ganttStartMin + (widget.tx + widget.viewportInnerW) / ganttPxPerMin).round();

    return Container(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [colorBg.withOpacity(0), colorBg],
          stops: const [0.0, 0.4],
        ),
      ),
      padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
      child: Row(
        children: [
          // Scrubber
          Expanded(
            child: LayoutBuilder(builder: (context, constraints) {
              final scrubberW = constraints.maxWidth;
              return Container(
                height: 22,
                decoration: BoxDecoration(
                  color: colorSurface1,
                  border: Border.all(color: colorDotted, width: 1.5),
                ),
                child: Stack(
                  clipBehavior: Clip.hardEdge,
                  children: [
                    // Fill
                    Positioned(
                      left: 0,
                      top: 0,
                      bottom: 0,
                      width: widget.progress * scrubberW,
                      child: Container(color: colorAccent.withOpacity(0.18)),
                    ),
                    // Head
                    Positioned(
                      left: (widget.progress * scrubberW - 1.5).clamp(0.0, scrubberW - 3),
                      top: -3,
                      bottom: -3,
                      width: 3,
                      child: Container(color: colorAccent),
                    ),
                    // Label
                    Positioned(
                      left: 8,
                      top: 0,
                      bottom: 0,
                      right: 8,
                      child: Center(
                        child: Text(
                          '${fmtTime(startMin)} → ${fmtTime(endMin)}',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            color: colorFg2,
                            height: 1,
                          ),
                        ),
                      ),
                    ),
                  ],
                ),
              );
            }),
          ),
          const SizedBox(width: 12),
          // Hint
          AnimatedBuilder(
            animation: _bobAnim,
            builder: (context, _) => Transform.translate(
              offset: Offset(0, _bobAnim.value),
              child: const Text(
                'scroll ↓',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.1 * 9,
                  color: colorFg3,
                  height: 1,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}
