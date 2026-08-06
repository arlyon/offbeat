// OFFBEAT — Member detail bottom sheet
// Avatar, name, last check-in, schedule preview, and membership controls
// Matches groups-screens.jsx MemberSheet (lines 787–857)

import 'package:flutter/material.dart';
import '../../data/group_presence.dart';
import '../../data/models.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../src/rust/api/dto.dart';

class MemberSheet extends StatelessWidget {
  final GroupMemberDto member;
  final String groupName;
  final LineupDto? lineup;
  final bool isMe;

  const MemberSheet({
    super.key,
    required this.member,
    required this.groupName,
    this.lineup,
    this.isMe = false,
  });

  Map<String, String> get _stageNames => {
    for (final stage in lineup?.stages ?? const <LineupStageDto>[])
      stage.id: stage.name,
  };

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      initialChildSize: 0.7,
      minChildSize: 0.4,
      maxChildSize: 0.95,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: Column(
          children: [
            // Grip
            Center(
              child: Container(
                margin: const EdgeInsets.only(top: 8),
                width: 36,
                height: 3,
                color: colorFg4,
              ),
            ),
            // Header
            _buildHeader(context),
            // Body
            Expanded(
              child: SingleChildScrollView(
                controller: scrollController,
                padding: const EdgeInsets.all(18),
                child: _buildBody(context),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildHeader(BuildContext context) {
    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 12, 18, 10),
        child: Row(
          children: [
            Expanded(
              child: Text.rich(
                TextSpan(
                  children: [
                    const TextSpan(text: 'MEMBER'),
                    const TextSpan(
                      text: '//',
                      style: TextStyle(color: colorAccent),
                    ),
                    TextSpan(text: groupName.toUpperCase()),
                  ],
                ),
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.08 * 11,
                  color: colorFg,
                ),
                overflow: TextOverflow.ellipsis,
              ),
            ),
            GestureDetector(
              onTap: () => Navigator.pop(context),
              child: const SizedBox(
                width: 28,
                height: 28,
                child: Center(
                  child: Icon(Icons.close, size: 16, color: colorFg2),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBody(BuildContext context) {
    final fresh = groupMemberIsOnSite(member);
    final stale = groupMemberIsStale(member);
    final hasLocation =
        groupMemberLocationKey(member) != groupPresenceOfflineKey;
    final presenceLabel = groupMemberPresenceLabel(member, _stageNames);
    final initials = _initials(member.displayName);
    final schedule = _resolvedSchedule;
    final missingSetIds = _missingSetIds;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        // Head row: avatar + name + status
        Row(
          crossAxisAlignment: CrossAxisAlignment.center,
          children: [
            // Big avatar
            Stack(
              clipBehavior: Clip.none,
              children: [
                Container(
                  width: 64,
                  height: 64,
                  color: colorSurface2,
                  child: Center(
                    child: Text(
                      initials,
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 22,
                        fontWeight: FontWeight.w700,
                        letterSpacing: -0.02 * 22,
                        color: fresh
                            ? colorAccent
                            : stale
                            ? colorWarn
                            : colorFg4,
                      ),
                    ),
                  ),
                ),
                if (hasLocation)
                  Positioned(
                    bottom: -4,
                    right: -4,
                    child: Container(
                      width: 12,
                      height: 12,
                      decoration: BoxDecoration(
                        color: stale ? colorWarn : colorAccent,
                        shape: BoxShape.circle,
                        border: Border.all(color: colorSurface1, width: 4),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(width: 14),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    member.displayName.toLowerCase(),
                    style: const TextStyle(
                      fontFamily: 'Helvetica',
                      fontWeight: FontWeight.w700,
                      fontSize: 24,
                      letterSpacing: -0.02 * 24,
                      height: 1.05,
                      color: colorFg,
                    ),
                  ),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      Container(
                        width: 7,
                        height: 7,
                        decoration: BoxDecoration(
                          color: stale
                              ? colorWarn
                              : fresh
                              ? colorAccent
                              : colorFg4,
                          shape: BoxShape.circle,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          presenceLabel,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            letterSpacing: 0.08 * 11,
                            color: stale
                                ? colorWarn
                                : fresh
                                ? colorAccent
                                : colorFg4,
                          ),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                  Text(
                    'JOINED THIS GROUP',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      letterSpacing: 0.08 * 10,
                      color: colorFg4,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        const SizedBox(height: 14),
        // Schedule section
        DottedBorder.top(
          child: Padding(
            padding: const EdgeInsets.only(top: 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  '\u2605 THEIR SCHEDULE \u00B7 ${member.starredSetIds.length}',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.08 * 10,
                    color: colorFg3,
                  ),
                ),
                const SizedBox(height: 8),
                if (member.starredSetIds.isEmpty)
                  const Padding(
                    padding: EdgeInsets.symmetric(vertical: 12),
                    child: Text(
                      'NO SHARED SETS',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 11,
                        letterSpacing: 0.08 * 11,
                        color: colorFg4,
                      ),
                    ),
                  )
                else ...[
                  for (final entry in schedule) _scheduleRow(entry),
                  for (final setId in missingSetIds)
                    _unavailableScheduleRow(setId),
                ],
              ],
            ),
          ),
        ),
        // Remove from group
        if (!isMe) ...[
          const SizedBox(height: 18),
          Center(
            child: GestureDetector(
              onTap: () {
                // Remove — future feature
              },
              child: const Padding(
                padding: EdgeInsets.symmetric(vertical: 10),
                child: Text(
                  'REMOVE FROM GROUP',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    letterSpacing: 0.08 * 10,
                    color: colorErr,
                  ),
                ),
              ),
            ),
          ),
        ],
      ],
    );
  }

  List<LineupSetDto> get _resolvedSchedule {
    final currentLineup = lineup;
    if (currentLineup == null) return const [];
    final byId = {for (final set in currentLineup.sets) set.id: set};
    final dayOrder = {
      for (var index = 0; index < currentLineup.days.length; index++)
        currentLineup.days[index].id: index,
    };
    final resolved = member.starredSetIds
        .map((setId) => byId[setId])
        .whereType<LineupSetDto>()
        .toList();
    resolved.sort((a, b) {
      final byDay = (dayOrder[a.day] ?? 1 << 20).compareTo(
        dayOrder[b.day] ?? 1 << 20,
      );
      return byDay != 0 ? byDay : a.startMin.compareTo(b.startMin);
    });
    return resolved;
  }

  List<String> get _missingSetIds {
    final available =
        lineup?.sets.map((set) => set.id).toSet() ?? const <String>{};
    final missing = member.starredSetIds
        .where((setId) => !available.contains(setId))
        .toList();
    missing.sort();
    return missing;
  }

  Widget _scheduleRow(LineupSetDto set) {
    final dayLabel = lineup?.days
        .where((day) => day.id == set.day)
        .map((day) => day.label)
        .firstOrNull;
    final stageName = lineup?.stages
        .where((stage) => stage.id == set.stage)
        .map((stage) => stage.name)
        .firstOrNull;
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Padding(
            padding: EdgeInsets.only(top: 2),
            child: Text(
              '★',
              style: TextStyle(color: colorAccent, fontSize: 11),
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  set.artist,
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontSize: 14,
                    fontWeight: FontWeight.w700,
                    color: colorFg,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  '${dayLabel ?? set.day} · ${fmtTime(set.startMin)} · ${stageName ?? set.stage}',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    letterSpacing: 0.04 * 9,
                    color: colorFg3,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _unavailableScheduleRow(String setId) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 10),
      child: Row(
        children: [
          const Text('?', style: TextStyle(color: colorWarn, fontSize: 11)),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              'SET UNAVAILABLE · ${setId.toUpperCase()}',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: 0.04 * 9,
                color: colorFg4,
              ),
              overflow: TextOverflow.ellipsis,
            ),
          ),
        ],
      ),
    );
  }

  String _initials(String name) {
    final parts = name.trim().split(RegExp(r'\s+'));
    if (parts.length >= 2) {
      return '${parts[0][0]}${parts[1][0]}'.toUpperCase();
    }
    return name.substring(0, name.length.clamp(0, 2)).toUpperCase();
  }
}
