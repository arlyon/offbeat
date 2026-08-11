import 'package:flutter/material.dart';

import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/chip.dart';
import '../../widgets/co_liker_pins.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/live_dot.dart';
import '../../widgets/star_button.dart';
import 'day_jump_strip.dart';

class StageTabsView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final void Function(String setId)? onStar;
  final void Function(Stage stage)? onStageChat;
  final ValueChanged<FestSet>? onSetTap;

  const StageTabsView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    this.onStar,
    this.onStageChat,
    this.onSetTap,
  });

  @override
  State<StageTabsView> createState() => _StageTabsViewState();
}

class _StageTabsViewState extends State<StageTabsView> {
  final _scrollController = ScrollController();
  final _listKey = GlobalKey();
  final _dayKeys = <String, GlobalKey>{};
  late String _activeStageId;
  late String _activeDayId;
  bool _syncScheduled = false;

  @override
  void initState() {
    super.initState();
    _activeStageId = widget.stages.first.id;
    _activeDayId = widget.days.first.id;
    _buildDayKeys();
    _scrollController.addListener(_scheduleActiveSync);
  }

  @override
  void didUpdateWidget(StageTabsView oldWidget) {
    super.didUpdateWidget(oldWidget);
    _buildDayKeys();
    if (!widget.stages.any((stage) => stage.id == _activeStageId)) {
      _activeStageId = widget.stages.first.id;
    }
    if (!widget.days.any((day) => day.id == _activeDayId)) {
      _activeDayId = widget.days.first.id;
    }
  }

  @override
  void dispose() {
    _scrollController
      ..removeListener(_scheduleActiveSync)
      ..dispose();
    super.dispose();
  }

  void _buildDayKeys() {
    for (final day in widget.days) {
      _dayKeys.putIfAbsent(day.id, GlobalKey.new);
    }
  }

  Map<String, List<FestSet>> get _setsByDay {
    final grouped = <String, List<FestSet>>{};
    for (final set in widget.sets.where((set) => set.stage == _activeStageId)) {
      (grouped[set.day] ??= []).add(set);
    }
    for (final sets in grouped.values) {
      sets.sort((a, b) => a.t.compareTo(b.t));
    }
    return grouped;
  }

  void _scheduleActiveSync() {
    if (_syncScheduled) return;
    _syncScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _syncScheduled = false;
      if (!mounted) return;
      _syncActiveDay();
    });
  }

  void _syncActiveDay() {
    final listRenderObject = _listKey.currentContext?.findRenderObject();
    if (listRenderObject is! RenderBox || !listRenderObject.attached) return;
    final listTop = listRenderObject.localToGlobal(Offset.zero).dy;
    String? closestDay;
    var closestDistance = double.infinity;

    for (final entry in _dayKeys.entries) {
      final renderObject = entry.value.currentContext?.findRenderObject();
      if (renderObject is! RenderBox || !renderObject.attached) continue;
      final y = renderObject.localToGlobal(Offset.zero).dy;
      final distance = (y - listTop).abs();
      if (distance < closestDistance) {
        closestDistance = distance;
        closestDay = entry.key;
      }
    }

    if (closestDay != null && closestDay != _activeDayId) {
      setState(() => _activeDayId = closestDay!);
    }
  }

  Future<void> _scrollToDay(String dayId) async {
    final context = _dayKeys[dayId]?.currentContext;
    if (context == null) return;
    await Scrollable.ensureVisible(
      context,
      duration: const Duration(milliseconds: 240),
      curve: curveBrutalist,
      alignment: 0,
    );
  }

  void _selectStage(String stageId) {
    if (stageId == _activeStageId) return;
    setState(() => _activeStageId = stageId);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _scrollToDay(_activeDayId);
    });
  }

  void _jumpToDay(String dayId) {
    setState(() => _activeDayId = dayId);
    _scrollToDay(dayId);
  }

  @override
  Widget build(BuildContext context) {
    final stage = widget.stages.firstWhere(
      (stage) => stage.id == _activeStageId,
    );
    return Column(
      children: [
        _JumpRow<Stage>(
          entries: widget.stages,
          activeId: _activeStageId,
          idOf: (stage) => stage.id,
          labelOf: (stage) => stage.name,
          onTap: _selectStage,
        ),
        DayJumpStrip(
          activeDayId: _activeDayId,
          days: widget.days,
          onDayTap: _jumpToDay,
        ),
        Expanded(
          child: ListView(
            key: _listKey,
            controller: _scrollController,
            padding: const EdgeInsets.only(bottom: 28),
            children: [
              _StageSection(
                stage: stage,
                days: widget.days,
                setsByDay: _setsByDay,
                dayKeys: _dayKeys,
                onStar: widget.onStar,
                onSetTap: widget.onSetTap,
                onStageChat: () => widget.onStageChat?.call(stage),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _JumpRow<T> extends StatelessWidget {
  final List<T> entries;
  final String activeId;
  final String Function(T entry) idOf;
  final String Function(T entry) labelOf;
  final ValueChanged<String> onTap;

  const _JumpRow({
    required this.entries,
    required this.activeId,
    required this.idOf,
    required this.labelOf,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: 44,
        child: ListView.separated(
          scrollDirection: Axis.horizontal,
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 7),
          itemCount: entries.length,
          separatorBuilder: (_, _) => const SizedBox(width: 6),
          itemBuilder: (context, index) {
            final entry = entries[index];
            final id = idOf(entry);
            return MonoChip(
              label: labelOf(entry).toUpperCase(),
              active: id == activeId,
              onTap: () => onTap(id),
            );
          },
        ),
      ),
    );
  }
}

class _StageSection extends StatelessWidget {
  final Stage stage;
  final List<Day> days;
  final Map<String, List<FestSet>> setsByDay;
  final Map<String, GlobalKey> dayKeys;
  final void Function(String setId)? onStar;
  final ValueChanged<FestSet>? onSetTap;
  final VoidCallback onStageChat;

  const _StageSection({
    required this.stage,
    required this.days,
    required this.setsByDay,
    required this.dayKeys,
    required this.onStageChat,
    this.onStar,
    this.onSetTap,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        DottedBorder.bottom(
          child: Container(
            color: colorSurface1,
            constraints: const BoxConstraints(minHeight: 58),
            padding: const EdgeInsets.only(left: 18, right: 8),
            child: Row(
              children: [
                Container(width: 12, height: 12, color: Color(stage.color)),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    stage.name.toUpperCase(),
                    style: const TextStyle(
                      fontFamily: 'Helvetica',
                      fontSize: 20,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -0.02 * 20,
                      color: colorFg,
                    ),
                  ),
                ),
                Semantics(
                  button: true,
                  label: 'Open chat for ${stage.name}',
                  child: InkWell(
                    onTap: onStageChat,
                    child: const SizedBox(
                      width: 48,
                      height: 48,
                      child: Icon(
                        Icons.chat_bubble_outline,
                        size: 18,
                        color: colorCoAccent,
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
        for (final day in days)
          _DaySection(
            key: dayKeys[day.id],
            day: day,
            stage: stage,
            sets: setsByDay[day.id] ?? const [],
            onStar: onStar,
            onSetTap: onSetTap,
          ),
      ],
    );
  }
}

class _DaySection extends StatelessWidget {
  final Day day;
  final Stage stage;
  final List<FestSet> sets;
  final void Function(String setId)? onStar;
  final ValueChanged<FestSet>? onSetTap;

  const _DaySection({
    super.key,
    required this.day,
    required this.stage,
    required this.sets,
    this.onStar,
    this.onSetTap,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Container(
          color: colorBg,
          padding: const EdgeInsets.fromLTRB(18, 12, 18, 8),
          child: Text(
            '${day.label.toUpperCase()} ${day.dayNum} ${day.month.toUpperCase()}',
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.08 * 10,
              color: colorFg2,
            ),
          ),
        ),
        if (sets.isEmpty)
          DottedBorder.bottom(
            child: const SizedBox(
              height: 44,
              child: Padding(
                padding: EdgeInsets.symmetric(horizontal: 18),
                child: Align(
                  alignment: Alignment.centerLeft,
                  child: Text(
                    'NO SETS AT THIS STAGE',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      letterSpacing: 0.08 * 9,
                      color: colorFg4,
                    ),
                  ),
                ),
              ),
            ),
          )
        else
          for (final set in sets)
            _SetRow(
              set: set,
              stage: stage,
              onStar: () => onStar?.call(set.id),
              onTap: () => onSetTap?.call(set),
            ),
      ],
    );
  }
}

class _SetRow extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final VoidCallback onStar;
  final VoidCallback onTap;

  const _SetRow({
    required this.set,
    required this.stage,
    required this.onStar,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Material(
        color: set.live ? colorAccentWash : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 72),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(18, 10, 12, 10),
              child: Row(
                children: [
                  SizedBox(
                    width: 58,
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          fmtTime(set.t),
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 13,
                            fontWeight: FontWeight.w700,
                            color: colorFg,
                          ),
                        ),
                        Text(
                          '→ ${fmtTime(set.t + set.dur)}',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            color: colorFg4,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Row(
                          children: [
                            if (set.live) ...[
                              const LiveDot(size: 7),
                              const SizedBox(width: 7),
                            ],
                            Expanded(
                              child: Text(
                                set.artist,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: const TextStyle(
                                  fontFamily: 'Helvetica',
                                  fontSize: 16,
                                  fontWeight: FontWeight.w700,
                                  height: 1.05,
                                  color: colorFg,
                                ),
                              ),
                            ),
                          ],
                        ),
                        if (set.genre.trim().isNotEmpty) ...[
                          const SizedBox(height: 3),
                          Text(
                            '${set.dur} MIN · ${set.genre.toUpperCase()}',
                            style: const TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 9,
                              letterSpacing: 0.06 * 9,
                              color: colorFg3,
                            ),
                          ),
                        ],
                        if (set.supporters.isNotEmpty)
                          CoLikerPins(
                            artist: set.artist,
                            supporters: set.supporters,
                          ),
                      ],
                    ),
                  ),
                  StarButton(starred: set.starred, onToggle: onStar, size: 20),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
