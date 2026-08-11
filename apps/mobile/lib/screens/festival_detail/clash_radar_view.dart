import 'package:flutter/material.dart';

import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import 'day_jump_strip.dart';

class ClashRadarView extends StatefulWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final ValueChanged<FestSet>? onSetTap;

  const ClashRadarView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    this.onSetTap,
  });

  @override
  State<ClashRadarView> createState() => _ClashRadarViewState();
}

class _ClashRadarViewState extends State<ClashRadarView> {
  late String _dayId;

  @override
  void initState() {
    super.initState();
    _dayId = widget.days.first.id;
  }

  @override
  void didUpdateWidget(ClashRadarView oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!widget.days.any((day) => day.id == _dayId)) {
      _dayId = widget.days.first.id;
    }
  }

  Map<String, Stage> get _stageById => {
    for (final stage in widget.stages) stage.id: stage,
  };

  List<FestSet> get _likedSets {
    final liked = widget.sets
        .where((set) => set.day == _dayId && set.starred)
        .toList();
    liked.sort((a, b) {
      final byTime = a.t.compareTo(b.t);
      return byTime != 0 ? byTime : a.artist.compareTo(b.artist);
    });
    return liked;
  }

  List<_AgendaGroup> _buildAgenda(List<FestSet> liked) {
    final byId = {for (final set in liked) set.id: set};
    final visited = <String>{};
    final groups = <_AgendaGroup>[];

    for (final set in liked) {
      if (!visited.add(set.id)) continue;
      final component = <FestSet>[];
      final pending = <FestSet>[set];

      while (pending.isNotEmpty) {
        final current = pending.removeLast();
        component.add(current);
        for (final clashId
            in current.cancelled ? const <String>[] : current.clashes) {
          final neighbour = byId[clashId];
          if (neighbour != null &&
              !neighbour.cancelled &&
              visited.add(neighbour.id)) {
            pending.add(neighbour);
          }
        }
      }

      component.sort((a, b) => a.t.compareTo(b.t));
      groups.add(_AgendaGroup(component));
    }

    groups.sort((a, b) => a.sets.first.t.compareTo(b.sets.first.t));
    return groups;
  }

  int _clashCount(List<FestSet> liked) {
    final likedIds = liked.map((set) => set.id).toSet();
    var count = 0;
    for (final set in liked) {
      count += set.clashes.where((id) {
        return likedIds.contains(id) && set.id.compareTo(id) < 0;
      }).length;
    }
    return count;
  }

  @override
  Widget build(BuildContext context) {
    final liked = _likedSets;
    final activeLiked = liked.where((set) => !set.cancelled).toList();
    final cancelledCount = liked.length - activeLiked.length;
    final groups = _buildAgenda(liked);
    final clashCount = _clashCount(activeLiked);
    final stageById = _stageById;

    return Column(
      children: [
        DayJumpStrip(
          activeDayId: _dayId,
          days: widget.days,
          onDayTap: (dayId) => setState(() => _dayId = dayId),
        ),
        DottedBorder.bottom(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(18, 13, 18, 13),
            child: Row(
              children: [
                Text(
                  '${activeLiked.length} ACTIVE'
                  '${cancelledCount == 0 ? '' : ' · $cancelledCount CANCELLED'}',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.08 * 11,
                    color: colorFg,
                  ),
                ),
                const Spacer(),
                Text(
                  '$clashCount CLASH${clashCount == 1 ? '' : 'ES'}',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w700,
                    letterSpacing: 0.08 * 10,
                    color: clashCount == 0 ? colorFg4 : colorWarn,
                  ),
                ),
              ],
            ),
          ),
        ),
        Expanded(
          child: liked.isEmpty
              ? const _EmptyLikedDay()
              : ListView.builder(
                  padding: const EdgeInsets.only(bottom: 32),
                  itemCount: groups.length,
                  itemBuilder: (context, index) {
                    final group = groups[index];
                    if (group.sets.length == 1) {
                      final set = group.sets.single;
                      return _LikedSetRow(
                        set: set,
                        stage: stageById[set.stage],
                        onTap: () => widget.onSetTap?.call(set),
                      );
                    }
                    return _ClashGroup(
                      sets: group.sets,
                      stages: stageById,
                      onSetTap: widget.onSetTap,
                    );
                  },
                ),
        ),
      ],
    );
  }
}

class _AgendaGroup {
  final List<FestSet> sets;

  const _AgendaGroup(this.sets);
}

class _EmptyLikedDay extends StatelessWidget {
  const _EmptyLikedDay();

  @override
  Widget build(BuildContext context) {
    return const Center(
      child: Padding(
        padding: EdgeInsets.symmetric(horizontal: 24),
        child: Text(
          'NO LIKED SETS THIS DAY\nSTAR SETS TO BUILD YOUR PLAN',
          textAlign: TextAlign.center,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 10,
            height: 1.5,
            letterSpacing: 0.08 * 10,
            color: colorFg3,
          ),
        ),
      ),
    );
  }
}

class _ClashGroup extends StatelessWidget {
  final List<FestSet> sets;
  final Map<String, Stage> stages;
  final ValueChanged<FestSet>? onSetTap;

  const _ClashGroup({required this.sets, required this.stages, this.onSetTap});

  String get _label {
    if (sets.length != 2) return '! ${sets.length} SETS CLASH';
    final first = sets[0];
    final second = sets[1];
    final overlapStart = first.t > second.t ? first.t : second.t;
    final firstEnd = first.t + first.dur;
    final secondEnd = second.t + second.dur;
    final overlapEnd = firstEnd < secondEnd ? firstEnd : secondEnd;
    return '! ${overlapEnd - overlapStart} MIN OVERLAP';
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      color: colorWarn.withValues(alpha: 0.04),
      child: DottedBorder.bottom(
        color: colorWarn,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(18, 12, 18, 6),
              child: Text(
                _label,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.08 * 10,
                  color: colorWarn,
                ),
              ),
            ),
            for (final set in sets)
              _LikedSetRow(
                set: set,
                stage: stages[set.stage],
                inClashGroup: true,
                onTap: () => onSetTap?.call(set),
              ),
          ],
        ),
      ),
    );
  }
}

class _LikedSetRow extends StatelessWidget {
  final FestSet set;
  final Stage? stage;
  final bool inClashGroup;
  final VoidCallback onTap;

  const _LikedSetRow({
    required this.set,
    required this.stage,
    required this.onTap,
    this.inClashGroup = false,
  });

  @override
  Widget build(BuildContext context) {
    final stageName = stage?.name ?? set.stage;
    final stageColor = stage == null ? colorFg3 : Color(stage!.color);
    return DottedBorder.bottom(
      color: inClashGroup ? colorWarn.withValues(alpha: 0.45) : colorDotted,
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 68),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(18, 9, 12, 9),
              child: Row(
                children: [
                  SizedBox(
                    width: 58,
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
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
                  Container(width: 3, height: 36, color: stageColor),
                  const SizedBox(width: 11),
                  Expanded(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          set.artist.toUpperCase(),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontFamily: 'Helvetica',
                            fontSize: 16,
                            fontWeight: FontWeight.w700,
                            letterSpacing: -0.02 * 16,
                            color: set.cancelled ? colorFg3 : colorFg,
                          ),
                        ),
                        const SizedBox(height: 3),
                        Text(
                          set.cancelled
                              ? 'CANCELLED · ${stageName.toUpperCase()}'
                              : stageName.toUpperCase(),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            fontWeight: set.cancelled
                                ? FontWeight.w700
                                : FontWeight.normal,
                            letterSpacing: 0.08 * 9,
                            color: set.cancelled ? colorWarn : colorFg3,
                          ),
                        ),
                      ],
                    ),
                  ),
                  const Icon(Icons.chevron_right, size: 18, color: colorFg3),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
