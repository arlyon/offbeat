import 'package:flutter/material.dart';

import '../../data/check_in_controller.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/co_liker_pins.dart';
import '../../widgets/dotted_border.dart';
import '../../widgets/live_dot.dart';
import '../check_in/check_in_band.dart';

class NowStripView extends StatelessWidget {
  final List<FestSet> sets;
  final List<Stage> stages;
  final List<Day> days;
  final DateTime now;
  final CheckInController? checkInController;
  final ValueChanged<FestSet>? onSetTap;
  final ValueChanged<Stage>? onStageChat;
  final VoidCallback? onGlobalChat;

  const NowStripView({
    super.key,
    required this.sets,
    required this.stages,
    required this.days,
    required this.now,
    this.checkInController,
    this.onSetTap,
    this.onStageChat,
    this.onGlobalChat,
  });

  Map<String, Stage> get _stageById => {
    for (final stage in stages) stage.id: stage,
  };

  Day? get _today {
    final today = DateTime(now.year, now.month, now.day);
    return days.where((day) => day.date == today).firstOrNull;
  }

  @override
  Widget build(BuildContext context) {
    final today = _today;
    final stageById = _stageById;
    final nowMin = now.hour * 60 + now.minute;
    final todaySets = today == null
        ? <FestSet>[]
        : sets.where((set) => set.day == today.id).toList();
    final current = todaySets.where((set) {
      return !set.cancelled && set.t <= nowMin && set.t + set.dur > nowMin;
    }).toList()..sort((a, b) => a.t.compareTo(b.t));
    final upcoming = todaySets.where((set) {
      return !set.cancelled && set.t > nowMin && set.t < nowMin + 240;
    }).toList()..sort((a, b) => a.t.compareTo(b.t));
    final nextLiked =
        todaySets.where((set) => set.starred && set.t > nowMin).toList()
          ..sort((a, b) => a.t.compareTo(b.t));

    return ListView(
      padding: EdgeInsets.zero,
      children: [
        _NowHero(
          current: current,
          stageById: stageById,
          nextLiked: nextLiked.firstOrNull,
          nowMin: nowMin,
          onSetTap: onSetTap,
          onGlobalChat: onGlobalChat,
        ),
        if (checkInController != null)
          CheckInBand(
            controller: checkInController!,
            stages: stages,
            sets: sets,
            groupCount: checkInController!.groupCount,
          ),
        DottedBorder.bottom(
          child: Container(
            color: colorSurface1,
            padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 8),
            child: const Row(
              children: [
                SizedBox(width: 56, child: Text('TIME', style: _depHeadStyle)),
                SizedBox(width: 12),
                SizedBox(width: 10),
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
        if (current.isEmpty && upcoming.isEmpty)
          const _NothingOn()
        else ...[
          for (final set in current)
            if (stageById[set.stage] case final stage?)
              _DepartureRow(
                set: set,
                stage: stage,
                status: 'ON NOW',
                statusColor: colorAccent,
                onTap: () => onSetTap?.call(set),
                onChat: () => onStageChat?.call(stage),
              ),
          for (final set in upcoming.take(8))
            if (stageById[set.stage] case final stage?)
              _DepartureRow(
                set: set,
                stage: stage,
                status: _statusFor(set, nowMin),
                statusColor: _statusColorFor(set, nowMin),
                onTap: () => onSetTap?.call(set),
              ),
        ],
        const SizedBox(height: 90),
      ],
    );
  }

  String _statusFor(FestSet set, int nowMin) {
    if (set.starred) return '★ LIKED';
    if (set.clashes.isNotEmpty) return '! CLASH';
    final minutes = set.t - nowMin;
    return minutes <= 15 ? 'T−${minutes}M' : 'UP NEXT';
  }

  Color _statusColorFor(FestSet set, int nowMin) {
    if (set.starred || set.t - nowMin <= 15) return colorAccent;
    if (set.clashes.isNotEmpty) return colorWarn;
    return colorFg4;
  }
}

class _NowHero extends StatelessWidget {
  final List<FestSet> current;
  final Map<String, Stage> stageById;
  final FestSet? nextLiked;
  final int nowMin;
  final ValueChanged<FestSet>? onSetTap;
  final VoidCallback? onGlobalChat;

  const _NowHero({
    required this.current,
    required this.stageById,
    required this.nextLiked,
    required this.nowMin,
    this.onSetTap,
    this.onGlobalChat,
  });

  @override
  Widget build(BuildContext context) {
    final primary = current.firstOrNull;
    final stage = primary == null ? null : stageById[primary.stage];
    final remaining = nextLiked == null ? null : nextLiked!.t - nowMin;

    return DottedBorder.bottom(
      child: Container(
        color: colorBg,
        padding: const EdgeInsets.fromLTRB(18, 16, 18, 20),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                if (current.isNotEmpty) ...[
                  const LiveDot(size: 7),
                  const SizedBox(width: 8),
                ],
                Expanded(
                  child: Text(
                    current.isEmpty
                        ? '// NOTHING ON NOW'
                        : '// ${current.length} ON NOW',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.08 * 10,
                      color: current.isEmpty ? colorFg4 : colorFg2,
                    ),
                  ),
                ),
                if (onGlobalChat != null)
                  Semantics(
                    button: true,
                    label: 'Open festival chat',
                    child: InkWell(
                      onTap: onGlobalChat,
                      child: const SizedBox(
                        height: 44,
                        child: Row(
                          children: [
                            Icon(
                              Icons.forum_outlined,
                              size: 16,
                              color: colorAccent,
                            ),
                            SizedBox(width: 7),
                            Text(
                              'CAMP CHAT',
                              style: TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 9,
                                fontWeight: FontWeight.w700,
                                letterSpacing: 0.06 * 9,
                                color: colorAccent,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
              ],
            ),
            if (primary != null) ...[
              const SizedBox(height: 8),
              Material(
                color: Colors.transparent,
                child: InkWell(
                  onTap: () => onSetTap?.call(primary),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          primary.artist,
                          style: const TextStyle(
                            fontFamily: 'Helvetica',
                            fontSize: 30,
                            fontWeight: FontWeight.w700,
                            letterSpacing: -0.02 * 30,
                            height: 1,
                            color: colorFg,
                          ),
                        ),
                        const SizedBox(height: 6),
                        Text(
                          '${stage?.name ?? 'STAGE'} · ${fmtTime(primary.t)} → '
                          '${fmtTime(primary.t + primary.dur)} · ${primary.genre}',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            letterSpacing: 0.06 * 10,
                            color: colorFg2,
                          ),
                        ),
                        if (primary.supporters.isNotEmpty)
                          CoLikerPins(
                            artist: primary.artist,
                            supporters: primary.supporters,
                          ),
                      ],
                    ),
                  ),
                ),
              ),
            ],
            if (nextLiked != null && remaining != null) ...[
              const SizedBox(height: 14),
              Text(
                'NEXT LIKED IN ${_durationLabel(remaining)} · ${nextLiked!.artist}',
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.06 * 10,
                  color: colorAccent,
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }

  String _durationLabel(int minutes) {
    final hours = minutes ~/ 60;
    final rest = minutes % 60;
    if (hours == 0) return '${rest}M';
    if (rest == 0) return '${hours}H';
    return '${hours}H ${rest}M';
  }
}

class _DepartureRow extends StatelessWidget {
  final FestSet set;
  final Stage stage;
  final String status;
  final Color statusColor;
  final VoidCallback onTap;
  final VoidCallback? onChat;

  const _DepartureRow({
    required this.set,
    required this.stage,
    required this.status,
    required this.statusColor,
    required this.onTap,
    this.onChat,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: Material(
        color: set.starred ? colorAccentWash : Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 68),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 10),
              child: Row(
                children: [
                  SizedBox(
                    width: 56,
                    child: Text(
                      fmtTime(set.t),
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 14,
                        fontWeight: FontWeight.w700,
                        color: colorFg,
                      ),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Container(width: 10, height: 10, color: Color(stage.color)),
                  const SizedBox(width: 12),
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
                            fontSize: 15,
                            fontWeight: FontWeight.w700,
                            color: colorFg,
                          ),
                        ),
                        const SizedBox(height: 3),
                        Text(
                          '${stage.name} · ${set.dur}M · ${set.genre}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            letterSpacing: 0.06 * 9,
                            color: colorFg3,
                          ),
                        ),
                      ],
                    ),
                  ),
                  SizedBox(
                    width: 70,
                    child: onChat == null
                        ? Text(
                            status,
                            textAlign: TextAlign.right,
                            style: TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 9,
                              fontWeight: FontWeight.w700,
                              color: statusColor,
                            ),
                          )
                        : Semantics(
                            button: true,
                            label: 'Open chat for ${stage.name}',
                            child: InkWell(
                              onTap: onChat,
                              child: const SizedBox(
                                height: 44,
                                child: Row(
                                  mainAxisAlignment: MainAxisAlignment.end,
                                  children: [
                                    Icon(
                                      Icons.chat_bubble_outline,
                                      size: 14,
                                      color: colorAccent,
                                    ),
                                    SizedBox(width: 5),
                                    Text(
                                      'CHAT',
                                      style: TextStyle(
                                        fontFamily: 'JetBrainsMono',
                                        fontSize: 9,
                                        fontWeight: FontWeight.w700,
                                        color: colorAccent,
                                      ),
                                    ),
                                  ],
                                ),
                              ),
                            ),
                          ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _NothingOn extends StatelessWidget {
  const _NothingOn();

  @override
  Widget build(BuildContext context) {
    return const Padding(
      padding: EdgeInsets.all(28),
      child: Center(
        child: Text(
          'NOTHING ELSE IN THE NEXT 4 HOURS',
          textAlign: TextAlign.center,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 10,
            fontWeight: FontWeight.w700,
            letterSpacing: 0.08 * 10,
            color: colorFg4,
          ),
        ),
      ),
    );
  }
}

const _depHeadStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.1 * 9,
  color: colorFg3,
);
