import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import '../../data/group_presence.dart';
import '../../src/rust/api/dto.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class GroupMembersSheet extends StatefulWidget {
  final List<GroupMemberDto> members;
  final ValueListenable<List<GroupMemberDto>>? membersListenable;
  final Map<String, String> stages;
  final String userId;
  final String? initialLocationKey;
  final ValueChanged<GroupMemberDto> onMemberTap;

  const GroupMembersSheet({
    super.key,
    required this.members,
    required this.stages,
    this.membersListenable,
    required this.userId,
    required this.onMemberTap,
    this.initialLocationKey,
  });

  @override
  State<GroupMembersSheet> createState() => _GroupMembersSheetState();
}

class _GroupMembersSheetState extends State<GroupMembersSheet> {
  String? _locationKey;

  @override
  void initState() {
    super.initState();
    _locationKey = widget.initialLocationKey;
  }

  List<GroupMemberDto> _visibleMembers(List<GroupMemberDto> source) {
    final members = source.where((member) {
      if (_locationKey == null) return true;
      return groupMemberLocationKey(member) == _locationKey;
    }).toList();
    members.sort((a, b) {
      if (a.userId == widget.userId) return -1;
      if (b.userId == widget.userId) return 1;
      if (groupMemberIsOnSite(a) && !groupMemberIsOnSite(b)) return -1;
      if (!groupMemberIsOnSite(a) && groupMemberIsOnSite(b)) return 1;
      return a.displayName.toLowerCase().compareTo(b.displayName.toLowerCase());
    });
    return members;
  }

  @override
  Widget build(BuildContext context) {
    final listenable = widget.membersListenable;
    if (listenable == null) return _buildSheet(context, widget.members);
    return ValueListenableBuilder<List<GroupMemberDto>>(
      valueListenable: listenable,
      builder: (context, members, _) => _buildSheet(context, members),
    );
  }

  Widget _buildSheet(BuildContext context, List<GroupMemberDto> allMembers) {
    final members = _visibleMembers(allMembers);
    final onSiteCount = allMembers.where(groupMemberIsOnSite).length;
    final filterMember = _locationKey == null
        ? null
        : allMembers
              .where((member) => groupMemberLocationKey(member) == _locationKey)
              .firstOrNull;
    final filterName = filterMember == null
        ? null
        : groupMemberLocationLabel(filterMember, widget.stages);

    return DraggableScrollableSheet(
      initialChildSize: 0.72,
      minChildSize: 0.42,
      maxChildSize: 0.92,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: SafeArea(
          top: false,
          child: Column(
            children: [
              Center(
                child: Container(
                  width: 36,
                  height: 3,
                  margin: const EdgeInsets.only(top: 8),
                  color: colorFg4,
                ),
              ),
              DottedBorder.bottom(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(18, 14, 18, 14),
                  child: Row(
                    children: [
                      const Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text('GROUP //', style: _accentMetaStyle),
                            SizedBox(height: 5),
                            Text(
                              'MEMBERS',
                              style: TextStyle(
                                fontFamily: 'Helvetica',
                                fontSize: 26,
                                fontWeight: FontWeight.w700,
                                letterSpacing: -0.02 * 26,
                                color: colorFg,
                              ),
                            ),
                          ],
                        ),
                      ),
                      Text(
                        '$onSiteCount ON SITE · ${allMembers.length} TOTAL',
                        style: _metaStyle,
                      ),
                    ],
                  ),
                ),
              ),
              if (filterName != null)
                DottedBorder.bottom(
                  child: InkWell(
                    onTap: () => setState(() => _locationKey = null),
                    child: SizedBox(
                      height: 44,
                      child: Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 18),
                        child: Row(
                          children: [
                            Expanded(
                              child: Text(
                                'FILTERED · $filterName',
                                style: _accentMetaStyle,
                              ),
                            ),
                            const Text('SHOW ALL ×', style: _metaStyle),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              Expanded(
                child: ListView.builder(
                  controller: scrollController,
                  itemCount: members.length,
                  itemBuilder: (context, index) {
                    final member = members[index];
                    final isMe = member.userId == widget.userId;
                    final onSite = groupMemberIsOnSite(member);
                    final hasLocation =
                        groupMemberLocationKey(member) !=
                        groupPresenceOfflineKey;
                    final locationLabel = groupMemberLocationLabel(
                      member,
                      widget.stages,
                    );
                    final checkInTime = groupMemberCheckInTime(member);
                    return DottedBorder.bottom(
                      child: Material(
                        color: isMe ? colorAccentWash : Colors.transparent,
                        child: InkWell(
                          onTap: () {
                            Navigator.pop(context);
                            widget.onMemberTap(member);
                          },
                          child: ConstrainedBox(
                            constraints: const BoxConstraints(minHeight: 56),
                            child: Padding(
                              padding: const EdgeInsets.symmetric(
                                horizontal: 18,
                              ),
                              child: Row(
                                children: [
                                  _MemberAvatar(member: member, isMe: isMe),
                                  const SizedBox(width: 12),
                                  Expanded(
                                    child: Column(
                                      mainAxisSize: MainAxisSize.min,
                                      crossAxisAlignment:
                                          CrossAxisAlignment.start,
                                      children: [
                                        Text(
                                          isMe ? 'YOU' : member.displayName,
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
                                        Text.rich(
                                          TextSpan(
                                            text: locationLabel,
                                            children: [
                                              if (checkInTime.isNotEmpty)
                                                TextSpan(
                                                  text: ' · $checkInTime',
                                                  style: const TextStyle(
                                                    color: colorFg4,
                                                  ),
                                                ),
                                            ],
                                          ),
                                          semanticsLabel:
                                              groupMemberPresenceLabel(
                                                member,
                                                widget.stages,
                                              ),
                                          maxLines: 1,
                                          overflow: TextOverflow.ellipsis,
                                          style: TextStyle(
                                            fontFamily: 'JetBrainsMono',
                                            fontSize: 9,
                                            fontWeight: FontWeight.w700,
                                            letterSpacing: 0.06 * 9,
                                            color: onSite
                                                ? colorCoAccent
                                                : hasLocation
                                                ? colorFg
                                                : colorFg4,
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                  Text(
                                    '${member.starredSetIds.length} LIKED',
                                    style: _metaStyle,
                                  ),
                                  const SizedBox(width: 6),
                                  const Icon(
                                    Icons.chevron_right,
                                    size: 18,
                                    color: colorFg3,
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                    );
                  },
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _MemberAvatar extends StatelessWidget {
  final GroupMemberDto member;
  final bool isMe;

  const _MemberAvatar({required this.member, required this.isMe});

  @override
  Widget build(BuildContext context) {
    final hasLocation =
        groupMemberLocationKey(member) != groupPresenceOfflineKey;
    final stale = groupMemberIsStale(member);
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Container(
          width: 40,
          height: 40,
          color: isMe ? colorAccent : colorSurface2,
          alignment: Alignment.center,
          child: Text(
            _initials(member.displayName),
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 12,
              fontWeight: FontWeight.w700,
              color: isMe ? colorAccentInk : colorFg,
            ),
          ),
        ),
        if (hasLocation)
          Positioned(
            right: -3,
            bottom: -3,
            child: Container(
              width: 10,
              height: 10,
              decoration: BoxDecoration(
                color: stale ? colorWarn : colorCoAccent,
                shape: BoxShape.circle,
                border: Border.all(color: colorSurface1, width: 2),
              ),
            ),
          ),
      ],
    );
  }
}

String _initials(String name) {
  final parts = name.trim().split(RegExp(r'\s+'));
  if (parts.length >= 2) return '${parts.first[0]}${parts[1][0]}'.toUpperCase();
  return name.substring(0, name.length.clamp(0, 2)).toUpperCase();
}

const _metaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.06 * 9,
  color: colorFg3,
);

const _accentMetaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.08 * 9,
  color: colorAccent,
);
