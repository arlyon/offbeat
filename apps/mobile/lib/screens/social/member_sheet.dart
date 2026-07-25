// OFFBEAT — Member detail bottom sheet
// Avatar, name, stage/offline status, schedule preview, DM/LOCATE, remove
// Matches groups-screens.jsx MemberSheet (lines 787–857)

import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../src/rust/api/dto.dart';

class MemberSheet extends StatelessWidget {
  final GroupMemberDto member;
  final String groupName;
  final bool isMe;

  const MemberSheet({
    super.key,
    required this.member,
    required this.groupName,
    this.isMe = false,
  });

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
    final live = member.stageId != null;
    final initials = _initials(member.displayName);

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
                        color: live ? colorAccent : colorFg4,
                      ),
                    ),
                  ),
                ),
                if (live)
                  Positioned(
                    bottom: -4,
                    right: -4,
                    child: Container(
                      width: 12,
                      height: 12,
                      decoration: BoxDecoration(
                        color: colorAccent,
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
                  if (live)
                    Row(
                      children: [
                        Container(
                          width: 7,
                          height: 7,
                          decoration: const BoxDecoration(
                            color: colorAccent,
                            shape: BoxShape.circle,
                          ),
                        ),
                        const SizedBox(width: 6),
                        Text(
                          '${member.stageId?.toUpperCase() ?? ''} ${member.customLocation != null ? '\u00B7 ${member.customLocation}' : ''}',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            letterSpacing: 0.08 * 11,
                            color: colorAccent,
                          ),
                        ),
                      ],
                    )
                  else
                    const Text(
                      '\u2014 OFFLINE',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 11,
                        letterSpacing: 0.08 * 11,
                        color: colorFg4,
                      ),
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
                const Text(
                  '\u2605 THEIR SCHEDULE \u00B7 NEXT 3',
                  style: TextStyle(
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
                else
                  ...member.starredSetIds
                      .take(3)
                      .map(
                        (setId) => Padding(
                          padding: const EdgeInsets.only(bottom: 6),
                          child: Row(
                            children: [
                              const Text(
                                '\u2605',
                                style: TextStyle(
                                  color: colorAccent,
                                  fontSize: 11,
                                ),
                              ),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  setId.toUpperCase(),
                                  style: const TextStyle(
                                    fontFamily: 'JetBrainsMono',
                                    fontSize: 11,
                                    letterSpacing: 0.04 * 11,
                                    color: colorFg2,
                                  ),
                                  overflow: TextOverflow.ellipsis,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 18),
        // DM / Locate buttons
        DottedBorder.top(
          child: Padding(
            padding: const EdgeInsets.only(top: 18),
            child: Row(
              children: [
                Expanded(
                  child: _ghostButton(
                    icon: Icons.chat_bubble_outline,
                    label: 'DM',
                    onTap: () {
                      // DM — future feature
                    },
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: _ghostButton(
                    icon: Icons.location_on_outlined,
                    label: 'LOCATE',
                    onTap: () {
                      // Locate — future feature
                    },
                  ),
                ),
              ],
            ),
          ),
        ),
        // Remove from group
        if (!isMe) ...[
          const SizedBox(height: 16),
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

  Widget _ghostButton({
    required IconData icon,
    required String label,
    required VoidCallback onTap,
  }) {
    return GestureDetector(
      onTap: onTap,
      child: DottedBorder(
        color: colorFg3,
        child: SizedBox(
          height: 44,
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, size: 13, color: colorFg),
              const SizedBox(width: 8),
              Text(
                label,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 11,
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.08 * 11,
                  color: colorFg,
                ),
              ),
            ],
          ),
        ),
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
