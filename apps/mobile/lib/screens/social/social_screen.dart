// OFFBEAT — Social/Groups screen
// Shows group identity header, members strip, pulse, feed, and composer.
// Empty state when no groups exist.

import 'dart:async';
import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../src/rust/api.dart';
import '../../src/rust/api/dto.dart';
import 'create_group_sheet.dart';
import 'invite_sheet.dart';
import 'member_sheet.dart';
import 'scan_sheet.dart';

class SocialScreen extends StatefulWidget {
  final AppNode node;
  final String festivalId;
  final String festivalName;
  final Map<String, String> stages;
  final String userId;
  final String? displayName;

  const SocialScreen({
    super.key,
    required this.node,
    required this.festivalId,
    required this.festivalName,
    required this.stages,
    required this.userId,
    this.displayName,
  });

  @override
  State<SocialScreen> createState() => _SocialScreenState();
}

class _SocialScreenState extends State<SocialScreen> {
  List<GroupInfo> _groups = [];
  bool _loading = true;
  String? _activeGroupId;

  // Group state (live)
  GroupStateDto? _groupState;
  StreamSubscription<GroupStateDto>? _groupStateSub;

  // Chat (live)
  List<ChatMessageDto> _messages = [];
  StreamSubscription<List<ChatMessageDto>>? _chatSub;

  // Peer count (live)
  int _directPeerCount = 0;
  StreamSubscription<List<PeerStatusInfo>>? _peerListSub;

  final _scrollController = ScrollController();
  final _composerController = TextEditingController();

  @override
  void initState() {
    super.initState();
    _loadGroups();
    _watchPeers();
  }

  @override
  void dispose() {
    _groupStateSub?.cancel();
    _chatSub?.cancel();
    _peerListSub?.cancel();
    _scrollController.dispose();
    _composerController.dispose();
    super.dispose();
  }

  Future<void> _loadGroups() async {
    setState(() => _loading = true);
    try {
      final groups = await widget.node.getGroups(festivalId: widget.festivalId);
      if (!mounted) return;
      setState(() {
        _groups = groups;
        _loading = false;
        if (groups.isNotEmpty) {
          _activeGroupId ??= groups.first.id;
          _subscribeToGroup(_activeGroupId!);
        }
      });
    } catch (e) {
      if (!mounted) return;
      setState(() => _loading = false);
    }
  }

  Future<void> _watchPeers() async {
    try {
      final stream = await widget.node.watchPeerList();
      _peerListSub = stream.listen((peers) {
        if (mounted) {
          final active = peers.where((p) => p.status == 'active').length;
          setState(() => _directPeerCount = active);
        }
      });
    } catch (_) {
      // Connection manager may not be available (e.g. in-memory node)
    }
  }

  Future<void> _subscribeToGroup(String groupId) async {
    _groupStateSub?.cancel();
    _chatSub?.cancel();

    try {
      final stateStream = await widget.node.watchGroupState(groupId: groupId);
      _groupStateSub = stateStream.listen((state) {
        if (!mounted) return;
        setState(() {
          _groupState = state;
          _groups = _groups
              .map(
                (group) => group.id == groupId && state.name.isNotEmpty
                    ? GroupInfo(id: group.id, name: state.name)
                    : group,
              )
              .toList();
        });
      });

      // Also get initial state
      final state = await widget.node.getGroupState(groupId: groupId);
      if (mounted) setState(() => _groupState = state);
    } catch (_) {}

    // Subscribe to chat
    try {
      final topic = 'group/$groupId/chat';
      final chatStream = await widget.node.watchChat(topic: topic, lastN: 50);
      _chatSub = chatStream.listen((msgs) {
        if (mounted) {
          setState(() => _messages = msgs);
          _scrollToBottom();
        }
      });

      // Load initial history
      final history = await widget.node.getChatHistory(
        topic: topic,
        limit: 50,
        offset: 0,
      );
      if (mounted) setState(() => _messages = history);
    } catch (_) {}
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollController.hasClients) {
        _scrollController.animateTo(
          _scrollController.position.maxScrollExtent,
          duration: const Duration(milliseconds: 140),
          curve: const Cubic(0.2, 0.7, 0.2, 1.0),
        );
      }
    });
  }

  Future<void> _sendMessage() async {
    final text = _composerController.text.trim();
    if (text.isEmpty || _activeGroupId == null) return;
    _composerController.clear();
    try {
      await widget.node.sendGroupChat(groupId: _activeGroupId!, text: text);
    } catch (_) {}
  }

  Future<void> _checkIn({String? stageId, String? customLocation}) async {
    final groupId = _activeGroupId;
    if (groupId == null) return;
    try {
      await widget.node.checkIn(
        groupId: groupId,
        stageId: stageId,
        customLocation: customLocation,
      );
      if (!mounted) return;
      Navigator.of(context).maybePop();
    } catch (_) {}
  }

  void _showCheckInSheet() {
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => _CheckInSheet(
        stages: widget.stages,
        onStage: (stageId) => _checkIn(stageId: stageId),
        onCustom: (location) => _checkIn(customLocation: location),
        onClear: () => _checkIn(),
      ),
    );
  }

  Future<void> _shareSchedule() async {
    final groupId = _activeGroupId;
    if (groupId == null) return;
    try {
      final stars = await widget.node.getStars(festivalId: widget.festivalId);
      await widget.node.updateSharedStars(groupId: groupId, setIds: stars);
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            stars.isEmpty
                ? 'SHARED SCHEDULE CLEARED'
                : '${stars.length} SAVED SETS SHARED',
          ),
        ),
      );
    } catch (_) {}
  }

  Future<void> _handleCreateGroup(String name) async {
    try {
      final result = await widget.node.createGroup(
        festivalId: widget.festivalId,
        name: name,
        displayName: widget.displayName ?? 'anon',
      );
      if (!mounted) return;
      setState(() {
        _activeGroupId = result.groupId;
      });
      await _loadGroups();
      _subscribeToGroup(result.groupId);
      // Show invite sheet after creating
      if (mounted) {
        _showInviteSheet(result.invitePayload);
      }
    } catch (_) {}
  }

  Future<void> _handleJoinGroup(String code) async {
    try {
      final result = await widget.node.joinGroup(
        invitePayload: code,
        displayName: widget.displayName ?? 'anon',
      );
      if (!mounted) return;
      setState(() {
        _activeGroupId = result.groupId;
      });
      await _loadGroups();
      _subscribeToGroup(result.groupId);
    } catch (_) {}
  }

  void _showCreateSheet() {
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => CreateGroupSheet(
        festivalName: widget.festivalName,
        onCreate: (name) {
          Navigator.pop(context);
          _handleCreateGroup(name);
        },
        onJoin: (code) {
          Navigator.pop(context);
          _handleJoinGroup(code);
        },
        onScanQr: () => _showScanSheet(),
      ),
    );
  }

  Future<void> _showInviteSheet([String? invitePayload]) async {
    if (_activeGroupId == null || _groupState == null) return;
    // Always use the full offbeat:// URI so the QR is scannable
    var code = invitePayload;
    code ??= await widget.node.getInvitePayload(
      groupId: _activeGroupId!,
      festivalId: widget.festivalId,
    );
    if (!mounted || code == null) return;
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => InviteSheet(
        groupName: _groupState!.name,
        groupCode: code!,
        festivalName: widget.festivalName,
      ),
    );
  }

  void _showScanSheet() {
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => ScanSheet(
        onScanned: (uri) {
          _handleJoinGroup(uri);
        },
      ),
    );
  }

  void _showMemberSheet(GroupMemberDto member) {
    if (_groupState == null) return;
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => MemberSheet(
        member: member,
        groupName: _groupState!.name,
        isMe: member.userId == widget.userId,
      ),
    );
  }

  void _switchGroup(String groupId) {
    setState(() {
      _activeGroupId = groupId;
      _groupState = null;
      _messages = [];
    });
    _subscribeToGroup(groupId);
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) {
      return const Center(
        child: CircularProgressIndicator(color: colorAccent, strokeWidth: 1.5),
      );
    }

    if (_groups.isEmpty) {
      return _buildEmptyState();
    }

    return Column(
      children: [
        Expanded(
          child: ListView(
            controller: _scrollController,
            children: [
              // Group identity header
              _buildGroupHeader(),
              // Members eyebrow + strip
              _buildMembersSection(),
              // Group pulse
              _buildPulseCard(),
              // Feed eyebrow
              _buildFeedEyebrow(),
              // Feed items (chat messages)
              ..._buildFeedItems(),
              const SizedBox(height: 10),
            ],
          ),
        ),
        // Composer
        _buildComposer(),
      ],
    );
  }

  Widget _buildEmptyState() {
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 48,
              height: 48,
              decoration: BoxDecoration(
                border: Border.all(color: colorDotted, width: 1.5),
              ),
              child: const Center(
                child: Icon(Icons.people_outline, color: colorAccent, size: 28),
              ),
            ),
            const SizedBox(height: 24),
            const Text(
              'NO GROUPS YET',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.1 * 11,
                color: colorFg,
                height: 1,
              ),
            ),
            const SizedBox(height: 12),
            const Text(
              'CREATE A GROUP TO COORDINATE\nWITH FRIENDS AT THE FESTIVAL',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: 0.08 * 9,
                color: colorFg3,
                height: 1.5,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 32),
            SizedBox(
              width: double.infinity,
              height: 48,
              child: Material(
                color: colorAccent,
                child: InkWell(
                  onTap: _showCreateSheet,
                  child: const Center(
                    child: Text(
                      'CREATE OR JOIN GROUP',
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
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  // ── Group identity header ─────────────────────────────────────

  Widget _buildGroupHeader() {
    final state = _groupState;
    final name =
        state?.name ??
        _groups
            .firstWhere(
              (g) => g.id == _activeGroupId,
              orElse: () => _groups.first,
            )
            .name;
    final memberCount = state?.members.length ?? 0;

    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 18, 18, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Group name with optional switcher
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Text(
                  name.toUpperCase(),
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontWeight: FontWeight.w700,
                    fontSize: 30,
                    letterSpacing: -0.02 * 30,
                    height: 0.98,
                    color: colorFg,
                  ),
                ),
              ),
              if (_groups.length > 1) _buildGroupSwitcher(),
            ],
          ),
          const SizedBox(height: 8),
          // Meta line
          Row(
            children: [
              Text('$memberCount MEMBERS', style: _metaStyle),
              _metaSep(),
              Text('$_directPeerCount DIRECT PEERS', style: _metaStyle),
              _metaSep(),
              Text(widget.festivalName.toUpperCase(), style: _metaStyle),
            ],
          ),
          const SizedBox(height: 14),
          // Action buttons row
          DottedBorder(
            child: Row(
              children: [
                _actionButton(
                  label: 'INVITE',
                  icon: Icons.person_add_alt_1,
                  primary: true,
                  onTap: () => _showInviteSheet(),
                ),
                _actionButton(
                  label: 'CHECK IN',
                  icon: Icons.location_on_outlined,
                  onTap: _showCheckInSheet,
                ),
                _actionButton(
                  label: 'SHARE ★',
                  icon: Icons.star_outline,
                  onTap: _shareSchedule,
                ),
                _actionButton(
                  label: 'NEW',
                  icon: Icons.add,
                  isLast: true,
                  onTap: _showCreateSheet,
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildGroupSwitcher() {
    return PopupMenuButton<String>(
      onSelected: _switchGroup,
      color: colorSurface1,
      shape: const RoundedRectangleBorder(),
      offset: const Offset(0, 36),
      itemBuilder: (_) => [
        ..._groups.map(
          (g) => PopupMenuItem<String>(
            value: g.id,
            child: Text(
              g.name.toUpperCase(),
              style: TextStyle(
                fontFamily: 'Helvetica',
                fontWeight: FontWeight.w700,
                fontSize: 14,
                letterSpacing: -0.01 * 14,
                color: g.id == _activeGroupId ? colorAccent : colorFg,
              ),
            ),
          ),
        ),
        PopupMenuItem<String>(
          value: '__create__',
          child: const Text(
            '+ NEW GROUP',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              fontWeight: FontWeight.w500,
              letterSpacing: 0.08 * 11,
              color: colorFg2,
            ),
          ),
        ),
      ],
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
        decoration: BoxDecoration(
          border: Border.all(color: colorDotted, width: 1.5),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              _groups
                  .firstWhere(
                    (g) => g.id == _activeGroupId,
                    orElse: () => _groups.first,
                  )
                  .name
                  .toUpperCase(),
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 10,
                fontWeight: FontWeight.w500,
                letterSpacing: 0.08 * 10,
                color: colorFg,
              ),
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(width: 6),
            const Text(
              '\u25BE',
              style: TextStyle(fontSize: 10, color: colorFg3),
            ),
          ],
        ),
      ),
    );
  }

  Widget _actionButton({
    required String label,
    required IconData icon,
    bool primary = false,
    bool isLast = false,
    required VoidCallback onTap,
  }) {
    return Expanded(
      child: GestureDetector(
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.symmetric(vertical: 11, horizontal: 12),
          decoration: BoxDecoration(
            color: primary ? colorAccent : Colors.transparent,
            border: isLast
                ? null
                : const Border(
                    right: BorderSide(color: colorDotted, width: 1.5),
                  ),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(icon, size: 13, color: primary ? colorAccentInk : colorFg),
              const SizedBox(width: 6),
              Text(
                label,
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  fontWeight: FontWeight.w500,
                  letterSpacing: 0.08 * 10,
                  color: primary ? colorAccentInk : colorFg,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  // ── Members ───────────────────────────────────────────────────

  Widget _buildMembersSection() {
    final members = _groupState?.members ?? [];
    final onSiteCount = members.where((m) => m.stageId != null).length;

    return Column(
      children: [
        _buildEyebrow('MEMBERS // $onSiteCount ON SITE', '\u2192 TAP TO VIEW'),
        DottedBorder(
          sides: const {DottedBorderSide.top, DottedBorderSide.bottom},
          child: SizedBox(
            height: 120,
            child: ListView.builder(
              scrollDirection: Axis.horizontal,
              itemCount: members.length + 1, // +1 for invite tile
              itemBuilder: (context, i) {
                if (i == members.length) {
                  return _buildInviteTile();
                }
                final m = members[i];
                final isMe = m.userId == widget.userId;
                return _buildMemberTile(m, isMe);
              },
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildMemberTile(GroupMemberDto m, bool isMe) {
    final live = m.stageId != null;
    final color = isMe ? colorAccent : (live ? colorStage2 : colorFg4);

    return GestureDetector(
      onTap: () => _showMemberSheet(m),
      child: Container(
        width: 92,
        padding: const EdgeInsets.fromLTRB(8, 12, 8, 12),
        decoration: BoxDecoration(
          color: isMe ? colorAccentWash : Colors.transparent,
          border: const Border(
            right: BorderSide(color: colorDotted, width: 1.5),
          ),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Avatar
            Stack(
              clipBehavior: Clip.none,
              children: [
                Container(
                  width: 44,
                  height: 44,
                  color: colorSurface2,
                  child: Center(
                    child: Text(
                      _initials(m.displayName),
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 14,
                        fontWeight: FontWeight.w700,
                        letterSpacing: -0.02 * 14,
                        color: live ? color : colorFg4,
                      ),
                    ),
                  ),
                ),
                if (live)
                  Positioned(
                    bottom: -3,
                    right: -3,
                    child: Container(
                      width: 10,
                      height: 10,
                      decoration: BoxDecoration(
                        color: colorAccent,
                        shape: BoxShape.circle,
                        border: Border.all(
                          color: isMe ? colorAccentWash : colorBg,
                          width: 3,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 8),
            // Name
            Text(
              isMe ? 'you' : m.displayName.toLowerCase(),
              style: const TextStyle(
                fontFamily: 'Helvetica',
                fontSize: 12,
                fontWeight: FontWeight.w700,
                letterSpacing: -0.01 * 12,
                height: 1.1,
                color: colorFg,
              ),
              overflow: TextOverflow.ellipsis,
            ),
            const SizedBox(height: 2),
            // Stage or offline
            Text(
              live
                  ? '\u00B0 ${m.stageId ?? ''}'.toUpperCase()
                  : '\u2014 OFFLINE',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: 0.06 * 9,
                color: live ? colorAccent : colorFg4,
                height: 1.2,
              ),
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildInviteTile() {
    return GestureDetector(
      onTap: () => _showInviteSheet(),
      child: Container(
        width: 92,
        padding: const EdgeInsets.fromLTRB(8, 12, 8, 12),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.start,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              width: 44,
              height: 44,
              decoration: BoxDecoration(
                border: Border.all(color: colorAccent, width: 1.5),
              ),
              child: const Center(
                child: Text(
                  '+',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 14,
                    fontWeight: FontWeight.w700,
                    color: colorAccent,
                  ),
                ),
              ),
            ),
            const SizedBox(height: 8),
            const Text(
              'invite',
              style: TextStyle(
                fontFamily: 'Helvetica',
                fontSize: 12,
                fontWeight: FontWeight.w700,
                letterSpacing: -0.01 * 12,
                height: 1.1,
                color: colorAccent,
              ),
            ),
            const SizedBox(height: 2),
            const Text(
              'QR \u00B7 CODE',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: 0.06 * 9,
                color: colorFg4,
                height: 1.2,
              ),
            ),
          ],
        ),
      ),
    );
  }

  // ── Pulse card ────────────────────────────────────────────────

  Widget _buildPulseCard() {
    final members = _groupState?.members ?? [];
    if (members.isEmpty) return const SizedBox.shrink();

    // Bucket members by stage (exclude self)
    final Map<String, List<GroupMemberDto>> buckets = {};
    for (final m in members) {
      if (m.userId == widget.userId) continue;
      final key = m.stageId ?? 'offline';
      buckets.putIfAbsent(key, () => []).add(m);
    }
    final totalOthers = members.length - 1;
    final sortedBuckets = buckets.entries.toList()
      ..sort((a, b) => b.value.length.compareTo(a.value.length));

    final now = DateTime.now();
    final timeStr =
        '${now.hour.toString().padLeft(2, '0')}:${now.minute.toString().padLeft(2, '0')}';

    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 14, 18, 16),
        child: Column(
          children: [
            _buildEyebrowInline(
              'GROUP PULSE // $timeStr',
              '${members.length} MEMBERS',
            ),
            ...sortedBuckets.map((entry) {
              final count = entry.value.length;
              final pct = totalOthers > 0
                  ? (count / totalOthers * 100).round()
                  : 0;
              final isOffline = entry.key == 'offline';
              return Padding(
                padding: const EdgeInsets.symmetric(vertical: 7),
                child: Row(
                  children: [
                    SizedBox(
                      width: 28,
                      child: Text(
                        '$count',
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 17,
                          fontWeight: FontWeight.w500,
                          letterSpacing: -0.02 * 17,
                          color: colorFg,
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    Container(
                      width: 4,
                      height: 14,
                      color: isOffline ? colorFg4 : colorStage2,
                    ),
                    const SizedBox(width: 5),
                    Expanded(
                      child: Text(
                        isOffline ? 'OFF GRID' : entry.key.toUpperCase(),
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          fontWeight: isOffline
                              ? FontWeight.w500
                              : FontWeight.w700,
                          letterSpacing: 0.08 * 10,
                          color: isOffline ? colorFg3 : colorFg,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                    ),
                    Text(
                      '$pct%',
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 9,
                        letterSpacing: 0.08 * 9,
                        color: colorFg4,
                      ),
                    ),
                  ],
                ),
              );
            }),
          ],
        ),
      ),
    );
  }

  // ── Feed ──────────────────────────────────────────────────────

  Widget _buildFeedEyebrow() {
    final name = _groupState?.name ?? '';
    final msgCount = _messages.length;
    return _buildEyebrow(
      'GROUP FEED // ${name.toUpperCase()}',
      '$msgCount MSGS',
    );
  }

  List<Widget> _buildFeedItems() {
    if (_messages.isEmpty) {
      return [
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 18, vertical: 24),
          child: Text(
            'NO MESSAGES YET \u2014 SAY SOMETHING',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              letterSpacing: 0.08 * 10,
              color: colorFg4,
            ),
            textAlign: TextAlign.center,
          ),
        ),
      ];
    }

    final widgets = <Widget>[];
    for (var i = 0; i < _messages.length; i++) {
      final msg = _messages[i];
      final prev = i > 0 ? _messages[i - 1] : null;
      final isMe = msg.userId == widget.userId;
      final consecutive = prev != null && prev.userId == msg.userId;

      widgets.add(_buildChatMsg(msg, isMe, consecutive));
    }
    return widgets;
  }

  Widget _buildChatMsg(ChatMessageDto msg, bool isMe, bool consecutive) {
    final ts = _parseTimestamp(msg.timestamp);
    final timeStr =
        '${ts.hour.toString().padLeft(2, '0')}:${ts.minute.toString().padLeft(2, '0')}';

    return Padding(
      padding: EdgeInsets.fromLTRB(18, consecutive ? 2 : 8, 18, 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (!isMe) ...[
            Opacity(
              opacity: consecutive ? 0 : 1,
              child: Container(
                width: 32,
                height: 32,
                color: colorSurface2,
                child: Center(
                  child: Text(
                    _initials(msg.displayName),
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -0.02 * 11,
                      color: colorFg,
                    ),
                  ),
                ),
              ),
            ),
            const SizedBox(width: 10),
          ],
          Expanded(
            child: Column(
              crossAxisAlignment: isMe
                  ? CrossAxisAlignment.end
                  : CrossAxisAlignment.start,
              children: [
                if (!consecutive)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 2),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          isMe ? 'you' : msg.displayName.toLowerCase(),
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            fontWeight: FontWeight.w500,
                            letterSpacing: 0.05 * 10,
                            color: colorFg,
                          ),
                        ),
                        const SizedBox(width: 8),
                        Text(
                          timeStr,
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            letterSpacing: 0.05 * 10,
                            color: colorFg4,
                          ),
                        ),
                      ],
                    ),
                  ),
                Text(
                  msg.text,
                  style: const TextStyle(
                    fontFamily: 'Helvetica',
                    fontSize: 14,
                    height: 1.35,
                    color: colorFg,
                  ),
                ),
              ],
            ),
          ),
          if (isMe) ...[
            const SizedBox(width: 10),
            Opacity(
              opacity: consecutive ? 0 : 1,
              child: Container(
                width: 32,
                height: 32,
                color: colorAccent,
                child: const Center(
                  child: Text(
                    'YOU',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      letterSpacing: -0.02 * 11,
                      color: colorAccentInk,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }

  // ── Composer ──────────────────────────────────────────────────

  Widget _buildComposer() {
    if (_groups.isEmpty) return const SizedBox.shrink();

    return DottedBorder.top(
      child: Container(
        color: colorBg,
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
        child: Row(
          children: [
            // Plus button
            GestureDetector(
              onTap: () {},
              child: Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(
                  border: Border.all(color: colorDotted, width: 1.5),
                ),
                child: const Center(
                  child: Icon(Icons.add, size: 16, color: colorFg2),
                ),
              ),
            ),
            const SizedBox(width: 8),
            // Text field
            Expanded(
              child: TextField(
                controller: _composerController,
                style: const TextStyle(
                  fontFamily: 'Helvetica',
                  fontSize: 14,
                  color: colorFg,
                  height: 1.3,
                ),
                decoration: const InputDecoration(
                  border: InputBorder.none,
                  hintText: 'message the group\u2026',
                  hintStyle: TextStyle(
                    fontFamily: 'Helvetica',
                    fontSize: 14,
                    color: colorFg4,
                  ),
                  contentPadding: EdgeInsets.symmetric(
                    horizontal: 4,
                    vertical: 8,
                  ),
                  isDense: true,
                ),
                onSubmitted: (_) => _sendMessage(),
              ),
            ),
            const SizedBox(width: 8),
            // Send button
            GestureDetector(
              onTap: _sendMessage,
              child: Container(
                width: 60,
                height: 36,
                color: colorSurface2,
                child: const Center(
                  child: Text(
                    'SEND',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 10,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.08 * 10,
                      color: colorFg4,
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }

  // ── Shared helpers ────────────────────────────────────────────

  Widget _buildEyebrow(String label, String meta) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(18, 14, 18, 8),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(
            label,
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              fontWeight: FontWeight.w500,
              letterSpacing: 0.08 * 11,
              color: colorFg3,
            ),
          ),
          Text(
            meta,
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              letterSpacing: 0.08 * 10,
              color: colorFg4,
            ),
          ),
        ],
      ),
    );
  }

  Widget _buildEyebrowInline(String label, String meta) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 4),
      child: Row(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          Text(
            label,
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              fontWeight: FontWeight.w500,
              letterSpacing: 0.08 * 11,
              color: colorFg3,
            ),
          ),
          Text(
            meta,
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              letterSpacing: 0.08 * 10,
              color: colorFg4,
            ),
          ),
        ],
      ),
    );
  }

  Widget _metaSep() {
    return const Padding(
      padding: EdgeInsets.symmetric(horizontal: 8),
      child: Text(
        '\u00B7',
        style: TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 10,
          color: colorFg4,
        ),
      ),
    );
  }

  static const _metaStyle = TextStyle(
    fontFamily: 'JetBrainsMono',
    fontSize: 10,
    fontWeight: FontWeight.w500,
    letterSpacing: 0.08 * 10,
    color: colorFg3,
  );

  String _initials(String name) {
    final parts = name.trim().split(RegExp(r'\s+'));
    if (parts.length >= 2) {
      return '${parts[0][0]}${parts[1][0]}'.toUpperCase();
    }
    return name.substring(0, name.length.clamp(0, 2)).toUpperCase();
  }

  DateTime _parseTimestamp(String ts) {
    try {
      return DateTime.parse(ts);
    } catch (_) {
      return DateTime.now();
    }
  }
}

class _CheckInSheet extends StatefulWidget {
  final Map<String, String> stages;
  final Future<void> Function(String stageId) onStage;
  final Future<void> Function(String location) onCustom;
  final Future<void> Function() onClear;

  const _CheckInSheet({
    required this.stages,
    required this.onStage,
    required this.onCustom,
    required this.onClear,
  });

  @override
  State<_CheckInSheet> createState() => _CheckInSheetState();
}

class _CheckInSheetState extends State<_CheckInSheet> {
  final _customController = TextEditingController();
  bool _saving = false;

  @override
  void dispose() {
    _customController.dispose();
    super.dispose();
  }

  Future<void> _run(Future<void> Function() action) async {
    if (_saving) return;
    setState(() => _saving = true);
    try {
      await action();
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final stages = widget.stages.entries.toList();
    return DraggableScrollableSheet(
      initialChildSize: 0.72,
      minChildSize: 0.45,
      maxChildSize: 0.92,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorSurface1,
        child: SafeArea(
          top: false,
          child: Column(
            children: [
              DottedBorder.bottom(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(18, 16, 12, 12),
                  child: Row(
                    children: [
                      const Expanded(
                        child: Text(
                          'CHECK IN//LOCATION',
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            fontWeight: FontWeight.w500,
                            letterSpacing: 0.08 * 11,
                            color: colorFg,
                          ),
                        ),
                      ),
                      IconButton(
                        onPressed: () => Navigator.pop(context),
                        icon: const Icon(
                          Icons.close,
                          size: 18,
                          color: colorFg2,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              Expanded(
                child: ListView(
                  controller: scrollController,
                  padding: const EdgeInsets.fromLTRB(18, 14, 18, 18),
                  children: [
                    const Text(
                      'STAGE',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        letterSpacing: 0.08 * 10,
                        color: colorFg3,
                      ),
                    ),
                    const SizedBox(height: 8),
                    if (stages.isEmpty)
                      const Padding(
                        padding: EdgeInsets.symmetric(vertical: 12),
                        child: Text(
                          'NO STAGES CACHED',
                          style: TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            color: colorFg4,
                          ),
                        ),
                      )
                    else
                      ...stages.map(
                        (stage) => GestureDetector(
                          onTap: _saving
                              ? null
                              : () => _run(() => widget.onStage(stage.key)),
                          child: DottedBorder.bottom(
                            child: SizedBox(
                              height: 48,
                              child: Row(
                                children: [
                                  const Icon(
                                    Icons.location_on_outlined,
                                    size: 16,
                                    color: colorAccent,
                                  ),
                                  const SizedBox(width: 10),
                                  Expanded(
                                    child: Text(
                                      stage.value.toUpperCase(),
                                      style: const TextStyle(
                                        fontFamily: 'JetBrainsMono',
                                        fontSize: 12,
                                        color: colorFg,
                                      ),
                                    ),
                                  ),
                                ],
                              ),
                            ),
                          ),
                        ),
                      ),
                    const SizedBox(height: 20),
                    const Text(
                      'CUSTOM LOCATION',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        letterSpacing: 0.08 * 10,
                        color: colorFg3,
                      ),
                    ),
                    const SizedBox(height: 8),
                    TextField(
                      controller: _customController,
                      enabled: !_saving,
                      style: const TextStyle(color: colorFg),
                      decoration: const InputDecoration(
                        hintText: 'CAMP, FOOD COURT, LANDMARK…',
                        hintStyle: TextStyle(color: colorFg4),
                        enabledBorder: UnderlineInputBorder(
                          borderSide: BorderSide(color: colorFg3),
                        ),
                        focusedBorder: UnderlineInputBorder(
                          borderSide: BorderSide(color: colorAccent),
                        ),
                      ),
                      onSubmitted: (value) {
                        final location = value.trim();
                        if (location.isNotEmpty) {
                          _run(() => widget.onCustom(location));
                        }
                      },
                    ),
                    const SizedBox(height: 12),
                    SizedBox(
                      height: 44,
                      child: OutlinedButton(
                        onPressed: _saving
                            ? null
                            : () {
                                final location = _customController.text.trim();
                                if (location.isNotEmpty) {
                                  _run(() => widget.onCustom(location));
                                }
                              },
                        child: const Text('CHECK IN HERE'),
                      ),
                    ),
                    const SizedBox(height: 8),
                    TextButton(
                      onPressed: _saving ? null : () => _run(widget.onClear),
                      child: const Text('CLEAR CHECK-IN'),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
