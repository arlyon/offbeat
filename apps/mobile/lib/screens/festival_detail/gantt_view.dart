// OFFBEAT GanttView — V1 Gantt-Scroll (THE signature view)
// Continuous timeline across all days; day chips jump to day start
// Stage labels (46px) sticky on left
// INTERACTION: scroll right side → horizontal time pan (haptic every 10min)
//              scroll left side → vertical stage row snap (haptic on snap)
// Set blocks: absolute positioned, 3px colored left border per stage
// Starred sets: accent-wash bg. Live sets: accent border glow
// NOW line: 2px accent vertical, 8px dot at top
// Bottom HUD: scrubber bar + "scroll ↓" hint with bob animation

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/live_dot.dart';
import '../../widgets/dotted_border.dart';

class GanttView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final DateTime now;

  const GanttView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    required this.now,
  });

  @override
  State<GanttView> createState() => _GanttViewState();
}

class _GanttViewState extends State<GanttView> {
  double _viewportInnerW = 0.0;

  // Horizontal scroll (right side sentinel — 1:1 mapping, 1px scroll = 1px pan)
  final ScrollController _hScrollController = ScrollController();

  // Vertical row scroll (left side sentinel)
  final ScrollController _vScrollController = ScrollController();
  static const double _minRowHeight = 64.0;
  static const double _timeAxisH = 26.0;
  static const double _hudPad = 44.0;

  // Haptic tracking for horizontal 10-min ticks
  int _lastHapticBucket = -1;

  // ── Cached data (recomputed only when widget.sets/days change) ──

  bool _hasComputedData = false;
  late Map<String, int> _dayOffsets;
  late List<FestSet> _absoluteSets;
  late int _startMin;
  late int _endMin;
  late double _contentW;
  late List<int> _axisHours;
  late int _absoluteNowMin;
  late double _nowX;
  late bool _nowInRange;

  /// Step graph: list of (startFrac, endFrac, normalizedHeight) for the
  /// scrubber activity overlay. Fractions are 0.0–1.0 across the full timeline.
  late List<(double, double, double)> _activitySteps;

  void _recomputeData() {
    // Capture current viewport center in absolute-time space so we can
    // restore it after the time range shifts.
    final hadData = _hasComputedData && _viewportInnerW > 0;
    final double prevCenterMin = hadData
        ? _startMin + (_tx + _viewportInnerW / 2) / ganttPxPerMin
        : -1;

    // Day offsets
    _dayOffsets = {
      for (int i = 0; i < widget.days.length; i++)
        widget.days[i].id: i * 24 * 60,
    };

    // Absolute-time sets
    _absoluteSets = widget.sets.map((s) {
      final offset = _dayOffsets[s.day] ?? 0;
      return s.copyWith(t: s.t + offset);
    }).toList();

    // Time range
    if (_absoluteSets.isEmpty) {
      _startMin = 0;
      _endMin = 24 * 60;
    } else {
      _startMin = (_absoluteSets.map((s) => s.t).reduce((a, b) => a < b ? a : b) ~/ 60) * 60;
      _endMin = ((_absoluteSets.map((s) => s.t + s.dur).reduce((a, b) => a > b ? a : b) + 59) ~/ 60) * 60;
    }

    _contentW = (_endMin - _startMin) * ganttPxPerMin;

    // Absolute "now" — map real DateTime into the multi-day timeline
    _absoluteNowMin = _resolveNowMin(widget.now, widget.days, _dayOffsets);
    _nowInRange = _absoluteNowMin >= _startMin && _absoluteNowMin <= _endMin;
    _nowX = (_absoluteNowMin - _startMin) * ganttPxPerMin;

    // Axis hours
    _axisHours = [for (int m = _startMin; m <= _endMin; m += 60) m];

    // Activity step graph — sweep line over set start/end events
    _activitySteps = _buildActivitySteps();

    _hasComputedData = true;

    // Restore scroll to same time position after recompute
    if (hadData && prevCenterMin >= 0 && _hScrollController.hasClients) {
      final target =
          ((prevCenterMin - _startMin) * ganttPxPerMin - _viewportInnerW / 2)
              .clamp(0.0, _maxTx);
      _hScrollController.jumpTo(target);
    }
  }

  List<(double, double, double)> _buildActivitySteps() {
    final range = _endMin - _startMin;
    if (range <= 0 || _absoluteSets.isEmpty) return const [];

    // Build sorted events: +1 at start, -1 at end
    final events = <(int, int)>[];
    for (final s in _absoluteSets) {
      events.add((s.t, 1));
      events.add((s.t + s.dur, -1));
    }
    events.sort((a, b) => a.$1 != b.$1 ? a.$1.compareTo(b.$1) : a.$2.compareTo(b.$2));

    // Walk events to build step function
    final steps = <(int, int, int)>[]; // (startMin, endMin, count)
    int count = 0;
    int prevT = events.first.$1;
    int maxCount = 0;

    for (final (t, delta) in events) {
      if (t != prevT && count > 0) {
        steps.add((prevT, t, count));
      }
      count += delta;
      if (count > maxCount) maxCount = count;
      prevT = t;
    }

    if (maxCount == 0) return const [];

    // Normalize to fractions
    return steps.map((s) {
      final (start, end, c) = s;
      return (
        (start - _startMin) / range,
        (end - _startMin) / range,
        c / maxCount,
      );
    }).toList();
  }

  // ── Scroll-derived values (cheap math, fine per frame) ─────

  double get _maxTx =>
      (_contentW - _viewportInnerW).clamp(0.0, double.infinity);
  double get _tx =>
      _hScrollController.hasClients ? _hScrollController.offset.clamp(0.0, _maxTx) : 0.0;
  double get _progress => _maxTx > 0 ? _tx / _maxTx : 0.0;

  String get _activeDay {
    final centerMin =
        _startMin + (_tx + _viewportInnerW / 2) / ganttPxPerMin;
    String active = widget.days.first.id;
    for (final day in widget.days) {
      if (centerMin >= _dayOffsets[day.id]!) active = day.id;
    }
    return active;
  }

  // ── Vertical row layout ────────────────────────────────────

  double _usableH(double stageAreaH) => stageAreaH - _hudPad;

  double _rowHeight(double stageAreaH) {
    final natural = _usableH(stageAreaH) / widget.stages.length;
    return natural < _minRowHeight ? _minRowHeight : natural;
  }

  bool _needsVertScroll(double stageAreaH) =>
      widget.stages.length * _minRowHeight > _usableH(stageAreaH);

  double _totalStagesHeight(double stageAreaH) =>
      widget.stages.length * _rowHeight(stageAreaH) + _hudPad;

  // ── Lifecycle ──────────────────────────────────────────────

  @override
  void initState() {
    super.initState();
    _recomputeData();
    _hScrollController.addListener(_onHScroll);
    _vScrollController.addListener(_onVScroll);
  }

  @override
  void didUpdateWidget(GanttView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_setsChanged(oldWidget) || _daysChanged(oldWidget) || oldWidget.now != widget.now) {
      _recomputeData();
    }
  }

  bool _setsChanged(GanttView old) {
    if (old.sets.length != widget.sets.length) return true;
    for (int i = 0; i < widget.sets.length; i++) {
      final a = old.sets[i], b = widget.sets[i];
      if (a.id != b.id || a.t != b.t || a.dur != b.dur || a.stage != b.stage ||
          a.day != b.day || a.starred != b.starred || a.live != b.live) {
        return true;
      }
    }
    return false;
  }

  bool _daysChanged(GanttView old) {
    if (old.days.length != widget.days.length) return true;
    for (int i = 0; i < widget.days.length; i++) {
      final a = old.days[i], b = widget.days[i];
      if (a.id != b.id || a.label != b.label || a.dayNum != b.dayNum || a.month != b.month || a.year != b.year) {
        return true;
      }
    }
    return false;
  }

  @override
  void dispose() {
    _hScrollController.removeListener(_onHScroll);
    _hScrollController.dispose();
    _vScrollController.removeListener(_onVScroll);
    _vScrollController.dispose();
    super.dispose();
  }

  void _onHScroll() {
    // No setState — ListenableBuilder on _hScrollController handles repaints.
    // Just fire haptics.
    final centerMin =
        _startMin + (_tx + _viewportInnerW / 2) / ganttPxPerMin;
    final bucket = (centerMin / 15).floor();
    if (bucket != _lastHapticBucket && _lastHapticBucket != -1) {
      bool crossedDay = false;
      if (widget.days.length > 1) {
        final lo = bucket < _lastHapticBucket ? bucket : _lastHapticBucket;
        final hi = bucket > _lastHapticBucket ? bucket : _lastHapticBucket;
        for (final day in widget.days) {
          final dayBucket = (_dayOffsets[day.id]! / 15).floor();
          if (dayBucket > lo && dayBucket <= hi) {
            crossedDay = true;
            break;
          }
        }
      }
      if (crossedDay) {
        HapticFeedback.heavyImpact();
      } else {
        HapticFeedback.lightImpact();
      }
    }
    _lastHapticBucket = bucket;
  }

  // No setState for vertical scroll either — ListenableBuilder handles it.
  void _onVScroll() {}

  void _snapVertical(double stageAreaH) {
    if (!_needsVertScroll(stageAreaH)) return;
    final rh = _rowHeight(stageAreaH);
    final offset = _vScrollController.offset;
    final nearestRow = (offset / rh).round() * rh;
    final clamped = nearestRow.clamp(
      0.0,
      _vScrollController.position.maxScrollExtent,
    );
    if ((clamped - offset).abs() > 0.5) {
      _vScrollController.animateTo(
        clamped,
        duration: const Duration(milliseconds: 120),
        curve: Curves.easeOut,
      );
      HapticFeedback.mediumImpact();
    }
  }

  void _jumpToDay(String dayId) {
    if (!_hScrollController.hasClients) return;
    final offsets = _dayOffsets;
    final dayOffset = offsets[dayId] ?? 0;

    // Jump to the earliest set on that day
    final daySets = _absoluteSets.where((s) => s.day == dayId);
    final dayStart = daySets.isEmpty
        ? dayOffset
        : daySets.map((s) => s.t).reduce((a, b) => a < b ? a : b);

    final target =
        ((dayStart - _startMin) * ganttPxPerMin).clamp(0.0, _maxTx);
    _hScrollController.animateTo(
      target,
      duration: const Duration(milliseconds: 300),
      curve: curveBrutalist,
    );
    HapticFeedback.mediumImpact();
  }

  void _centerOnNow() {
    if (_viewportInnerW <= 0 || !_hScrollController.hasClients) return;
    final target = (_nowX - _viewportInnerW / 2).clamp(0.0, _maxTx);
    _hScrollController.jumpTo(target);
  }

  // ── Build ──────────────────────────────────────────────────

  @override
  Widget build(BuildContext context) {
    final allSets = _absoluteSets;

    return Column(
      children: [
        // Meta strip: now time + scrollable day jump chips
        ListenableBuilder(
          listenable: _hScrollController,
          builder: (context, _) => _MetaStrip(
            activeDay: _activeDay,
            days: widget.days,
            showDayPicker: widget.days.length > 1,
            onDayTap: _jumpToDay,
            nowInRange: _nowInRange,
            absoluteNowMin: _absoluteNowMin,
            nowMinOfDay: widget.now.hour * 60 + widget.now.minute,
            startMin: _startMin,
            endMin: _endMin,
          ),
        ),
        // Gantt viewport
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              final vw = constraints.maxWidth - ganttStageLabelW;
              final stageAreaH = constraints.maxHeight - _timeAxisH;
              final rh = _rowHeight(stageAreaH);
              final needsVScroll = _needsVertScroll(stageAreaH);
              final totalStagesH = _totalStagesHeight(stageAreaH);

              if (_viewportInnerW != vw) {
                final wasZero = _viewportInnerW == 0.0;
                _viewportInnerW = vw;
                if (wasZero) {
                  WidgetsBinding.instance.addPostFrameCallback((_) {
                    _centerOnNow();
                  });
                }
              }

              return Stack(
                children: [
                  // Gantt content
                  Positioned.fill(
                      child: IgnorePointer(
                        child: RepaintBoundary(
                          child: ListenableBuilder(
                            listenable: Listenable.merge([_hScrollController, _vScrollController]),
                            builder: (context, _) => _GanttContent(
                              tx: _tx,
                              progress: _progress,
                              viewportInnerW: _viewportInnerW,
                              allSets: allSets,
                              stages: widget.stages,
                              days: widget.days,
                              dayOffsets: _dayOffsets,
                              axisHours: _axisHours,
                              nowX: _nowX,
                              nowInRange: _nowInRange,
                              startMin: _startMin,
                              vertOffset: _vScrollController.hasClients
                                  ? _vScrollController.offset
                                  : 0.0,
                              rowHeight: rh,
                            ),
                          ),
                        ),
                      ),
                    ),
                  // Horizontal scroll sentinel (always present so controller attaches)
                  Positioned.fill(
                    child: SingleChildScrollView(
                      controller: _hScrollController,
                      child: SizedBox(
                        width: 1,
                        height: _maxTx + constraints.maxHeight,
                      ),
                    ),
                  ),
                  // Vertical row scroll sentinel (left side, overlays horizontal)
                  if (needsVScroll)
                    Positioned(
                      left: 0,
                      width: ganttStageLabelW,
                      top: _timeAxisH,
                      bottom: 0,
                      child: NotificationListener<ScrollEndNotification>(
                        onNotification: (_) {
                          _snapVertical(stageAreaH);
                          return false;
                        },
                        child: SingleChildScrollView(
                          controller: _vScrollController,
                          child: SizedBox(
                            width: ganttStageLabelW,
                            height: totalStagesH,
                          ),
                        ),
                      ),
                    ),
                  // Bottom HUD
                  Positioned(
                      left: 0,
                      right: 0,
                      bottom: 0,
                      child: ListenableBuilder(
                        listenable: _hScrollController,
                        builder: (context, _) => _GanttHUD(
                          progress: _progress,
                          tx: _tx,
                          viewportInnerW: _viewportInnerW,
                          startMin: _startMin,
                          maxTx: _maxTx,
                          activitySteps: _activitySteps,
                          onScrub: (p) {
                            final target = (p * _maxTx).clamp(0.0, _maxTx);
                            _hScrollController.jumpTo(target);
                          },
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

// ── Meta strip ─────────────────────────────────────────────────

class _MetaStrip extends StatelessWidget {
  final String activeDay;
  final List<Day> days;
  final bool showDayPicker;
  final ValueChanged<String> onDayTap;
  final bool nowInRange;
  final int absoluteNowMin;
  final int nowMinOfDay;
  final int startMin;
  final int endMin;

  const _MetaStrip({
    required this.activeDay,
    required this.days,
    required this.onDayTap,
    required this.nowInRange,
    required this.absoluteNowMin,
    required this.nowMinOfDay,
    required this.startMin,
    required this.endMin,
    this.showDayPicker = true,
  });

  String get _nowLabel {
    if (nowInRange) return '// NOW';
    if (absoluteNowMin < startMin) {
      final diff = startMin - absoluteNowMin;
      final h = diff ~/ 60;
      final m = diff % 60;
      return h > 0 ? '// STARTS IN ${h}H${m > 0 ? ' ${m}M' : ''}' : '// STARTS IN ${m}M';
    }
    return '// ENDED';
  }

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
                    Text(
                      _nowLabel,
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 9,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.1 * 9,
                        color: nowInRange ? colorFg3 : colorFg4,
                        height: 1,
                      ),
                    ),
                    if (nowInRange) ...[
                      const SizedBox(width: 6),
                      const LiveDot(size: 6),
                    ],
                  ],
                ),
                const SizedBox(height: 2),
                Text(
                  fmtTime(nowMinOfDay),
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 12,
                    color: nowInRange ? colorFg : colorFg4,
                    height: 1,
                  ),
                ),
              ],
            ),
            const SizedBox(width: 14),
            // Scrollable day jump chips
            if (showDayPicker)
              Expanded(
                child: SizedBox(
                  height: 28,
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: days.map((d) {
                        final isActive = d.id == activeDay;
                        return Padding(
                          padding: const EdgeInsets.only(right: 4),
                          child: GestureDetector(
                            onTap: () => onDayTap(d.id),
                            child: Container(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 10,
                                vertical: 6,
                              ),
                              decoration: BoxDecoration(
                                color:
                                    isActive ? colorFg : Colors.transparent,
                                border: Border.all(
                                  color: isActive ? colorFg : colorDotted,
                                  width: 1.5,
                                ),
                              ),
                              child: Text(
                                '${d.label} ${d.dayNum}',
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
                  ),
                ),
              )
            else
              const Spacer(),
          ],
        ),
      ),
    );
  }
}

// ── Gantt content ──────────────────────────────────────────────

class _GanttContent extends StatelessWidget {
  final double tx;
  final double progress;
  final double viewportInnerW;
  final List<FestSet> allSets;
  final List<Stage> stages;
  final List<Day> days;
  final Map<String, int> dayOffsets;
  final List<int> axisHours;
  final double nowX;
  final bool nowInRange;
  final int startMin;
  final double vertOffset;
  final double rowHeight;

  const _GanttContent({
    required this.tx,
    required this.progress,
    required this.viewportInnerW,
    required this.allSets,
    required this.stages,
    required this.days,
    required this.dayOffsets,
    required this.axisHours,
    required this.nowX,
    required this.nowInRange,
    required this.startMin,
    required this.vertOffset,
    required this.rowHeight,
  });

  @override
  Widget build(BuildContext context) {
    // Visible time window in minutes (with padding for partially visible blocks)
    final visMinStart = startMin + (tx - 200) / ganttPxPerMin;
    final visMinEnd = startMin + (tx + viewportInnerW + 200) / ganttPxPerMin;

    // Pre-bucket sets by stage, filtered to visible viewport only
    final setsByStage = <String, List<FestSet>>{};
    for (final s in allSets) {
      final setEnd = s.t + s.dur;
      if (setEnd < visMinStart || s.t > visMinEnd) continue;
      (setsByStage[s.stage] ??= []).add(s);
    }

    // Filter axis hours to visible range
    const tickW = 60.0 * ganttPxPerMin;
    final visibleHours = axisHours.where((m) {
      final x = (m - startMin) * ganttPxPerMin + ganttStageLabelW - tx;
      return x + tickW > 0 && x < viewportInnerW + ganttStageLabelW;
    }).toList();

    return Column(
      children: [
        _TimeAxis(
          tx: tx,
          visibleHours: visibleHours,
          startMin: startMin,
          days: days,
          dayOffsets: dayOffsets,
        ),
        Expanded(
          child: _StageRows(
            tx: tx,
            setsByStage: setsByStage,
            stages: stages,
            nowX: nowX,
            nowInRange: nowInRange,
            startMin: startMin,
            vertOffset: vertOffset,
            rowHeight: rowHeight,
          ),
        ),
      ],
    );
  }
}

// ── Time axis ──────────────────────────────────────────────────

class _TimeAxis extends StatelessWidget {
  final double tx;
  final List<int> visibleHours;
  final int startMin;
  final List<Day> days;
  final Map<String, int> dayOffsets;

  const _TimeAxis({
    required this.tx,
    required this.visibleHours,
    required this.startMin,
    required this.days,
    required this.dayOffsets,
  });

  // Pre-compute day boundary set for O(1) lookup
  Set<int> get _dayBoundaryHours {
    if (days.length <= 1) return const {};
    return {
      for (final off in dayOffsets.values)
        if ((off ~/ 60) * 60 > startMin) (off ~/ 60) * 60,
    };
  }

  @override
  Widget build(BuildContext context) {
    const tickW = 60.0 * ganttPxPerMin;
    final dayBounds = _dayBoundaryHours;

    return DottedBorder.bottom(
      child: SizedBox(
        height: 26,
        child: Stack(
          clipBehavior: Clip.hardEdge,
          children: [
            // Only render visible hour ticks, absolutely positioned
            for (final m in visibleHours)
              () {
                final x = (m - startMin) * ganttPxPerMin + ganttStageLabelW - tx;
                final isDayStart = dayBounds.contains(m);
                return Positioned(
                  left: x,
                  top: 0,
                  bottom: 0,
                  width: tickW,
                  child: Stack(
                    children: [
                      if (isDayStart)
                        Positioned(
                          left: 0,
                          top: 0,
                          bottom: 0,
                          child: Container(width: 2, color: colorFg3),
                        ),
                      Padding(
                        padding: const EdgeInsets.only(left: 10, top: 6),
                        child: Text(
                          fmtTime(m),
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            color: isDayStart ? colorFg : colorFg2,
                            fontWeight: isDayStart
                                ? FontWeight.w700
                                : FontWeight.normal,
                            height: 1,
                          ),
                        ),
                      ),
                      Positioned(
                        right: 0,
                        top: 0,
                        bottom: 0,
                        child: Container(width: 1, color: colorDotted),
                      ),
                      Positioned(
                        left: tickW / 2,
                        top: 0,
                        bottom: 0,
                        child: Container(width: 1, color: colorHairline),
                      ),
                    ],
                  ),
                );
              }(),
          ],
        ),
      ),
    );
  }
}

// ── Stage rows ─────────────────────────────────────────────────

class _StageRows extends StatelessWidget {
  final double tx;
  final Map<String, List<FestSet>> setsByStage;
  final List<Stage> stages;
  final double nowX;
  final bool nowInRange;
  final int startMin;
  final double vertOffset;
  final double rowHeight;

  const _StageRows({
    required this.tx,
    required this.setsByStage,
    required this.stages,
    required this.nowX,
    required this.nowInRange,
    required this.startMin,
    required this.vertOffset,
    required this.rowHeight,
  });

  @override
  Widget build(BuildContext context) {
    return Stack(
      clipBehavior: Clip.hardEdge,
      children: [
        // Stage rows (clipped, translated vertically)
        Positioned.fill(
          child: ClipRect(
            child: OverflowBox(
              alignment: Alignment.topLeft,
              maxHeight: double.infinity,
              child: Transform.translate(
                offset: Offset(0, -vertOffset),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    ...stages.asMap().entries.map((entry) {
                      final i = entry.key;
                      final stage = entry.value;
                      return SizedBox(
                        height: rowHeight,
                        child: _SingleStageRow(
                          stage: stage,
                          sets: setsByStage[stage.id] ?? const [],
                          tx: tx,
                          isLast: i == stages.length - 1,
                          startMin: startMin,
                        ),
                      );
                    }),
                    SizedBox(height: _GanttViewState._hudPad),
                  ],
                ),
              ),
            ),
          ),
        ),
        // NOW line (only when current time is within the festival timeline)
        if (nowInRange)
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

// ── Single stage row ───────────────────────────────────────────

class _SingleStageRow extends StatelessWidget {
  final Stage stage;
  final List<FestSet> sets;
  final double tx;
  final bool isLast;
  final int startMin;

  const _SingleStageRow({
    required this.stage,
    required this.sets,
    required this.tx,
    required this.isLast,
    required this.startMin,
  });

  @override
  Widget build(BuildContext context) {
    final stageColor = Color(stage.color);
    return Stack(
      clipBehavior: Clip.hardEdge,
      children: [
        if (!isLast)
          Positioned(
              bottom: 0, left: 0, right: 0, child: const DottedRule()),
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
                colors: [colorBg, colorBg.withValues(alpha: 0)],
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
                Container(width: 14, height: 3, color: stageColor),
              ],
            ),
          ),
        ),
        // Set blocks (already filtered to visible viewport)
        for (final s in sets)
          Positioned(
            left: (s.t - startMin) * ganttPxPerMin + ganttStageLabelW - tx,
            top: 6,
            bottom: 6,
            width: s.dur * ganttPxPerMin,
            child: _SetBlock(set: s, stageColor: stageColor),
          ),
      ],
    );
  }
}

// ── Set block ──────────────────────────────────────────────────

class _SetBlock extends StatelessWidget {
  final FestSet set;
  final Color stageColor;

  const _SetBlock({required this.set, required this.stageColor});

  @override
  Widget build(BuildContext context) {
    BoxDecoration decoration = BoxDecoration(
      color: colorSurface1,
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
                const Text(
                  '★ ',
                  style:
                      TextStyle(color: colorAccent, fontSize: 10, height: 1),
                ),
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

// ── Bottom HUD ─────────────────────────────────────────────────

class _GanttHUD extends StatefulWidget {
  final double progress;
  final double tx;
  final double viewportInnerW;
  final int startMin;
  final double maxTx;
  final List<(double, double, double)> activitySteps;
  final ValueChanged<double> onScrub;

  const _GanttHUD({
    required this.progress,
    required this.tx,
    required this.viewportInnerW,
    required this.startMin,
    required this.maxTx,
    required this.activitySteps,
    required this.onScrub,
  });

  @override
  State<_GanttHUD> createState() => _GanttHUDState();
}

class _GanttHUDState extends State<_GanttHUD>
    with SingleTickerProviderStateMixin {
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
    final startMin = (widget.startMin + widget.tx / ganttPxPerMin).round();
    final endMin =
        (widget.startMin +
                (widget.tx + widget.viewportInnerW) / ganttPxPerMin)
            .round();

    return Container(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [colorBg.withValues(alpha: 0), colorBg],
          stops: const [0.0, 0.4],
        ),
      ),
      padding: const EdgeInsets.fromLTRB(14, 10, 14, 10),
      child: Row(
        children: [
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final scrubberW = constraints.maxWidth;
                return GestureDetector(
                  onTapDown: (d) =>
                      widget.onScrub((d.localPosition.dx / scrubberW).clamp(0.0, 1.0)),
                  onHorizontalDragUpdate: (d) =>
                      widget.onScrub((d.localPosition.dx / scrubberW).clamp(0.0, 1.0)),
                  child: Container(
                    height: 22,
                    decoration: BoxDecoration(
                      color: colorSurface1,
                      border: Border.all(color: colorDotted, width: 1.5),
                    ),
                    child: Stack(
                      clipBehavior: Clip.hardEdge,
                      children: [
                        // Activity step graph
                        Positioned.fill(
                          child: CustomPaint(
                            painter: _ActivityPainter(widget.activitySteps),
                          ),
                        ),
                        // Progress fill
                        Positioned(
                          left: 0,
                          top: 0,
                          bottom: 0,
                          width: widget.progress * scrubberW,
                          child: Container(
                            color: colorAccent.withValues(alpha: 0.18),
                          ),
                        ),
                        Positioned(
                          left: (widget.progress * scrubberW - 1.5).clamp(
                            0.0,
                            scrubberW - 3,
                          ),
                          top: -3,
                          bottom: -3,
                          width: 3,
                          child: Container(color: colorAccent),
                        ),
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
                  ),
                );
              },
            ),
          ),
          const SizedBox(width: 12),
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

/// Map a real [DateTime] into the Gantt timeline's absolute-minute space.
///
/// Matches [now] against [days] by day-of-month + month abbreviation.
/// Returns minutes-since-midnight offset by the matching day's position.
/// If today isn't a festival day, places "now" before the first day or
/// after the last day depending on the calendar date.
/// Parse a [Day] into a [DateTime] (midnight on that day).
DateTime _dayToDate(Day d) {
  const months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
  return DateTime(d.year, months.indexOf(d.month) + 1, int.parse(d.dayNum));
}

int _resolveNowMin(DateTime now, List<Day> days, Map<String, int> dayOffsets) {
  const months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
  final nowMonth = months[now.month - 1];
  final nowDay = now.day.toString();
  final nowMinOfDay = now.hour * 60 + now.minute;

  // Exact match — today is a festival day
  for (final d in days) {
    if (d.year == now.year && d.dayNum == nowDay && d.month == nowMonth) {
      return (dayOffsets[d.id] ?? 0) + nowMinOfDay;
    }
  }

  if (days.isEmpty) return nowMinOfDay;

  // Not a festival day — determine if we're before or after
  final nowDate = DateTime(now.year, now.month, now.day);
  final firstDayDate = _dayToDate(days.first);

  if (nowDate.isBefore(firstDayDate)) {
    final daysBefore = firstDayDate.difference(nowDate).inDays;
    return (dayOffsets[days.first.id] ?? 0) - daysBefore * 24 * 60 + nowMinOfDay;
  }

  final lastDayDate = _dayToDate(days.last);
  final daysAfter = nowDate.difference(lastDayDate).inDays;
  return (dayOffsets[days.last.id] ?? 0) + daysAfter * 24 * 60 + nowMinOfDay;
}

class _ActivityPainter extends CustomPainter {
  final List<(double, double, double)> steps;

  _ActivityPainter(this.steps);

  @override
  void paint(Canvas canvas, Size size) {
    if (steps.isEmpty) return;
    final paint = Paint()..color = colorFg3.withValues(alpha: 0.18);
    for (final (startFrac, endFrac, height) in steps) {
      final x = startFrac * size.width;
      final w = (endFrac - startFrac) * size.width;
      final h = height * size.height;
      canvas.drawRect(Rect.fromLTWH(x, size.height - h, w, h), paint);
    }
  }

  @override
  bool shouldRepaint(_ActivityPainter old) => old.steps != steps;
}
