// OFFBEAT — Social/Groups screen
// Shows group identity header, members strip, pulse, feed, and composer.
// Empty state when no groups exist.

import 'dart:async';
import 'package:flutter/material.dart';
import '../../data/check_in_controller.dart';
import '../../data/group_presence.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import '../../src/rust/api.dart';
import '../../src/rust/api/dto.dart';
import 'create_group_sheet.dart';
import 'group_members_sheet.dart';
import 'invite_sheet.dart';
import 'member_sheet.dart';
import 'scan_sheet.dart';

class SocialActionsController {
  VoidCallback? _openGroupActions;

  void openGroupActions() => _openGroupActions?.call();
}

class SocialScreen extends StatefulWidget {
  final AppNode node;
  final String festivalId;
  final String festivalName;
  final Map<String, String> stages;
  final String userId;
  final String? displayName;
  final LineupDto? lineup;
  final VoidCallback? onGroupsChanged;
  final SocialActionsController? actionsController;
  final CheckInController? checkInController;

  const SocialScreen({
    super.key,
    required this.node,
    required this.festivalId,
    required this.festivalName,
    required this.stages,
    required this.userId,
    this.displayName,
    this.lineup,
    this.onGroupsChanged,
    this.actionsController,
    this.checkInController,
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
  int _groupSubscriptionGeneration = 0;

  // Chat (live)
  List<ChatMessageDto> _messages = [];
  StreamSubscription<List<ChatMessageDto>>? _chatSub;

  final _scrollController = ScrollController();
  final _composerController = TextEditingController();

  @override
  void initState() {
    super.initState();
    widget.actionsController?._openGroupActions = _showGroupActions;
    _loadGroups();
  }

  @override
  void didUpdateWidget(SocialScreen oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.actionsController != widget.actionsController) {
      oldWidget.actionsController?._openGroupActions = null;
      widget.actionsController?._openGroupActions = _showGroupActions;
    }
  }

  @override
  void dispose() {
    widget.actionsController?._openGroupActions = null;
    _groupSubscriptionGeneration++;
    _groupStateSub?.cancel();
    _chatSub?.cancel();
    _scrollController.dispose();
    _composerController.dispose();
    super.dispose();
  }

  Future<void> _loadGroups({String? preferredGroupId}) async {
    if (_groups.isEmpty) setState(() => _loading = true);
    try {
      final groups = await widget.node.getGroups(festivalId: widget.festivalId);
      if (!mounted) return;
      final existingIds = groups.map((group) => group.id).toSet();
      final nextGroupId =
          preferredGroupId != null && existingIds.contains(preferredGroupId)
          ? preferredGroupId
          : existingIds.contains(_activeGroupId)
          ? _activeGroupId
          : groups.firstOrNull?.id;
      setState(() {
        _groups = groups;
        _loading = false;
        _activeGroupId = nextGroupId;
        if (nextGroupId == null) {
          _groupState = null;
          _messages = [];
        }
      });
      if (nextGroupId != null) {
        await _subscribeToGroup(nextGroupId);
      }
    } catch (_) {
      if (!mounted) return;
      setState(() => _loading = false);
    }
  }

  Future<void> _subscribeToGroup(String groupId) async {
    final generation = ++_groupSubscriptionGeneration;
    await _groupStateSub?.cancel();
    await _chatSub?.cancel();
    _groupStateSub = null;
    _chatSub = null;
    if (!mounted || generation != _groupSubscriptionGeneration) return;

    try {
      final stateStream = await widget.node.watchGroupState(groupId: groupId);
      if (!mounted ||
          generation != _groupSubscriptionGeneration ||
          _activeGroupId != groupId) {
        await stateStream.listen((_) {}).cancel();
        return;
      }
      _groupStateSub = stateStream.listen((state) {
        if (!mounted ||
            generation != _groupSubscriptionGeneration ||
            _activeGroupId != groupId) {
          return;
        }
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

      final state = await widget.node.getGroupState(groupId: groupId);
      if (mounted &&
          generation == _groupSubscriptionGeneration &&
          _activeGroupId == groupId) {
        setState(() => _groupState = state);
      }
    } catch (_) {}

    try {
      final topic = 'group/$groupId/chat';
      final chatStream = await widget.node.watchChat(topic: topic, lastN: 50);
      if (!mounted ||
          generation != _groupSubscriptionGeneration ||
          _activeGroupId != groupId) {
        await chatStream.listen((_) {}).cancel();
        return;
      }
      _chatSub = chatStream.listen((msgs) {
        if (!mounted ||
            generation != _groupSubscriptionGeneration ||
            _activeGroupId != groupId) {
          return;
        }
        setState(() => _messages = msgs);
        _scrollToBottom();
      });

      final history = await widget.node.getChatHistory(
        topic: topic,
        limit: 50,
        offset: 0,
      );
      if (mounted &&
          generation == _groupSubscriptionGeneration &&
          _activeGroupId == groupId) {
        setState(() => _messages = history);
      }
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
    if (mounted) setState(() {});
    try {
      await widget.node.sendGroupChat(groupId: _activeGroupId!, text: text);
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
        _groups = [
          ..._groups.where((group) => group.id != result.groupId),
          GroupInfo(id: result.groupId, name: name),
        ];
        _activeGroupId = result.groupId;
        _groupState = null;
        _messages = [];
        _loading = false;
      });
      await _loadGroups(preferredGroupId: result.groupId);
      await widget.checkInController?.refresh();
      widget.onGroupsChanged?.call();
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
        festivalId: widget.festivalId,
        displayName: widget.displayName ?? 'anon',
      );
      if (!mounted) return;
      await _loadGroups(preferredGroupId: result.groupId);
      await widget.checkInController?.refresh();
      widget.onGroupsChanged?.call();
    } catch (_) {}
  }

  Future<void> _leaveActiveGroup() async {
    final groupId = _activeGroupId;
    if (groupId == null) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        shape: const RoundedRectangleBorder(),
        backgroundColor: colorSurface1,
        title: const Text('LEAVE GROUP?'),
        content: const Text(
          'YOUR LIKES WILL STOP SYNCING. LOCAL SAVED SETS ARE KEPT.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('CANCEL'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('LEAVE', style: TextStyle(color: colorErr)),
          ),
        ],
      ),
    );
    if (confirmed != true) return;

    try {
      await widget.node.leaveGroup(groupId: groupId);
      _groupSubscriptionGeneration++;
      await _groupStateSub?.cancel();
      await _chatSub?.cancel();
      _groupStateSub = null;
      _chatSub = null;
      if (!mounted) return;
      _activeGroupId = null;
      _groupState = null;
      _messages = [];
      widget.onGroupsChanged?.call();
      await _loadGroups();
    } catch (_) {
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(const SnackBar(content: Text('COULD NOT LEAVE GROUP')));
    }
  }

  void _showCreateSheet({String initialTab = 'new'}) {
    showModalBottomSheet(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => CreateGroupSheet(
        festivalName: widget.festivalName,
        initialTab: initialTab,
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

  void _showGroupActions() {
    if (!mounted) return;
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (sheetContext) => _GroupActionsSheet(
        groups: _groups,
        activeGroupId: _activeGroupId,
        onSwitch: (groupId) {
          Navigator.pop(sheetContext);
          _switchGroup(groupId);
        },
        onCreate: () {
          Navigator.pop(sheetContext);
          _showCreateSheet();
        },
        onJoin: () {
          Navigator.pop(sheetContext);
          _showCreateSheet(initialTab: 'join');
        },
        onLeave: _activeGroupId == null
            ? null
            : () {
                Navigator.pop(sheetContext);
                _leaveActiveGroup();
              },
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

  void _showMembersSheet({String? locationKey}) {
    final state = _groupState;
    if (state == null) return;
    showModalBottomSheet<void>(
      context: context,
      backgroundColor: Colors.transparent,
      isScrollControlled: true,
      builder: (_) => GroupMembersSheet(
        members: state.members,
        stages: widget.stages,
        userId: widget.userId,
        initialLocationKey: locationKey,
        onMemberTap: (member) {
          Future<void>.delayed(Duration.zero, () {
            if (mounted) _showMemberSheet(member);
          });
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
        lineup: widget.lineup,
        isMe: member.userId == widget.userId,
      ),
    );
  }

  Future<void> _switchGroup(String groupId) async {
    setState(() {
      _activeGroupId = groupId;
      _groupState = null;
      _messages = [];
    });
    await _subscribeToGroup(groupId);
  }

  @override
  Widget build(BuildContext context) {
    if (_loading) return _buildLoadingState();

    if (_groups.isEmpty) {
      return _buildEmptyState();
    }

    return Column(
      children: [
        Expanded(
          child: ListView(
            controller: _scrollController,
            children: [
              _buildGroupHeader(),
              _buildMembersSection(),
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

  Widget _buildLoadingState() {
    return Padding(
      padding: const EdgeInsets.all(18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(width: 190, height: 24, color: colorSurface2),
          const SizedBox(height: 10),
          Container(width: 260, height: 10, color: colorSurface2),
          const SizedBox(height: 28),
          DottedBorder(
            child: SizedBox(
              height: 88,
              child: Row(
                children: List.generate(
                  4,
                  (index) => Padding(
                    padding: const EdgeInsets.only(left: 12),
                    child: Container(
                      width: 44,
                      height: 44,
                      color: colorSurface2,
                    ),
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(height: 18),
          Container(width: double.infinity, height: 48, color: colorSurface2),
        ],
      ),
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
              'YOUR CREW ISN’T HERE YET',
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
            Row(
              children: [
                Expanded(
                  child: _emptyAction(
                    label: 'CREATE',
                    primary: true,
                    onTap: () => _showCreateSheet(),
                  ),
                ),
                const SizedBox(width: 8),
                Expanded(
                  child: _emptyAction(
                    label: 'JOIN',
                    onTap: () => _showCreateSheet(initialTab: 'join'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }

  Widget _emptyAction({
    required String label,
    required VoidCallback onTap,
    bool primary = false,
  }) {
    return Material(
      color: primary ? colorAccent : Colors.transparent,
      child: InkWell(
        onTap: onTap,
        child: Container(
          height: 48,
          decoration: primary
              ? null
              : BoxDecoration(
                  border: Border.all(color: colorDotted, width: 1.5),
                ),
          alignment: Alignment.center,
          child: Text(
            label,
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.1 * 10,
              color: primary ? colorAccentInk : colorFg,
            ),
          ),
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
                    fontSize: 25,
                    letterSpacing: -0.02 * 25,
                    height: 0.98,
                    color: colorFg,
                  ),
                ),
              ),
              _buildInviteAction(),
            ],
          ),
          const SizedBox(height: 8),
          // Meta line
          Row(
            children: [
              Text('$memberCount MEMBERS', style: _metaStyle),
              _metaSep(),
              Text('$_directPeerCount DIRECT PEERS', style: _metaStyle),
            ],
          ),
        ],
      ),
    );
  }

  Widget _buildInviteAction() {
    return Semantics(
      button: true,
      label: 'Invite to group',
      child: InkWell(
        onTap: () => _showInviteSheet(),
        child: const SizedBox(
          width: 52,
          height: 44,
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Text(
                '+',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 18,
                  fontWeight: FontWeight.w700,
                  color: colorAccent,
                  height: 0.8,
                ),
              ),
              SizedBox(height: 5),
              Text('INVITE', style: _accentMetaStyle),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildCheckInBar() {
    final controller = widget.checkInController;
    if (controller == null) return const SizedBox.shrink();
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final checkIn = controller.checkIn;
        final label = switch (checkIn?.kind) {
          'campsite' => 'CAMPSITE',
          'stage' => widget.stages[checkIn?.value]?.toUpperCase() ?? 'STAGE',
          'custom' => checkIn?.value?.toUpperCase() ?? 'CUSTOM LOCATION',
          _ => 'NOT CHECKED IN',
        };
        return DottedBorder(
          sides: const {DottedBorderSide.top, DottedBorderSide.bottom},
          child: ListTile(
            leading: Icon(
              checkIn == null ? Icons.location_off_outlined : Icons.location_on,
              color: checkIn == null ? colorFg4 : colorAccent,
            ),
            title: const Text('YOU', style: _metaStyle),
            subtitle: Text(label),
            trailing: TextButton(
              onPressed: controller.saving
                  ? null
                  : () => showCheckInSheet(
                      context,
                      controller: controller,
                      stages: widget.scheduleStages,
                      sets: widget.scheduleSets,
                    ),
              child: Text(checkIn == null ? 'CHECK IN' : 'UPDATE'),
            ),
          ),
        );
      },
    );
  }

  // ── Members ───────────────────────────────────────────────────

  Widget _buildMembersSection() {
    final members = _groupState?.members ?? const <GroupMemberDto>[];
    final onSiteCount = members
        .where((member) => member.stageId != null)
        .length;
    final visible = members.take(5).toList();
    final stackWidth = visible.isEmpty
        ? 44.0
        : 44.0 + (visible.length - 1) * 28.0;

    return DottedBorder.top(
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: _showMembersSheet,
          child: ConstrainedBox(
            constraints: const BoxConstraints(minHeight: 88),
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 12),
              child: Row(
                children: [
                  SizedBox(
                    width: stackWidth,
                    height: 44,
                    child: Stack(
                      children: [
                        for (var index = 0; index < visible.length; index++)
                          Positioned(
                            left: index * 28,
                            child: _StackedAvatar(
                              member: visible[index],
                              isMe: visible[index].userId == widget.userId,
                              overflow: index == visible.length - 1
                                  ? members.length - visible.length
                                  : 0,
                            ),
                          ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Column(
                      mainAxisSize: MainAxisSize.min,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '$onSiteCount ON SITE · ${members.length} MEMBERS',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            fontWeight: FontWeight.w700,
                            color: colorFg,
                          ),
                        ),
                        const SizedBox(height: 4),
                        const Text('TAP TO VIEW THE CREW', style: _metaStyle),
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

  // ── Pulse card ────────────────────────────────────────────────

  Widget _buildPulseCard() {
    final members = _groupState?.members ?? const <GroupMemberDto>[];
    if (members.isEmpty) return const SizedBox.shrink();

    final buckets = <String, List<GroupMemberDto>>{};
    for (final member in members) {
      final key = member.stageId ?? 'offline';
      (buckets[key] ??= []).add(member);
    }
    final sortedBuckets = buckets.entries.toList()
      ..sort((a, b) {
        if (a.key == 'offline') return 1;
        if (b.key == 'offline') return -1;
        return b.value.length.compareTo(a.value.length);
      });
    final visibleBuckets = sortedBuckets.take(3).toList();

    return DottedBorder.bottom(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 12, 18, 14),
        child: Column(
          children: [
            _buildEyebrowInline('WHERE THE CREW IS', 'TAP A LOCATION'),
            for (final entry in visibleBuckets)
              Material(
                color: Colors.transparent,
                child: InkWell(
                  onTap: () => _showMembersSheet(stageId: entry.key),
                  child: SizedBox(
                    height: 44,
                    child: Row(
                      children: [
                        SizedBox(
                          width: 28,
                          child: Text(
                            '${entry.value.length}',
                            style: const TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 16,
                              fontWeight: FontWeight.w700,
                              color: colorFg,
                            ),
                          ),
                        ),
                        Container(
                          width: 8,
                          height: 8,
                          color: entry.key == 'offline'
                              ? colorFg4
                              : colorCoAccent,
                        ),
                        const SizedBox(width: 10),
                        Expanded(
                          child: Text(
                            entry.key == 'offline'
                                ? 'OFF GRID'
                                : (widget.stages[entry.key] ?? entry.key)
                                      .toUpperCase(),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 10,
                              fontWeight: FontWeight.w700,
                              color: entry.key == 'offline'
                                  ? colorFg3
                                  : colorFg,
                            ),
                          ),
                        ),
                        Text(
                          entry.value
                              .take(2)
                              .map(
                                (member) => member.displayName.split(' ').first,
                              )
                              .join(' · ')
                              .toUpperCase(),
                          style: _metaStyle,
                        ),
                        const SizedBox(width: 4),
                        const Icon(
                          Icons.chevron_right,
                          size: 16,
                          color: colorFg3,
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            if (sortedBuckets.length > visibleBuckets.length)
              Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  '+${sortedBuckets.length - visibleBuckets.length} MORE LOCATIONS',
                  style: _metaStyle,
                ),
              ),
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
      'GROUP CHAT // ${name.toUpperCase()}',
      '$msgCount MSGS',
    );
  }

  List<Widget> _buildFeedItems() {
    if (_messages.isEmpty) {
      return [
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 18, vertical: 24),
          child: Text(
            'NO MESSAGES YET · START THE CHAT',
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
    final canSend = _composerController.text.trim().isNotEmpty;

    return DottedBorder.top(
      child: Container(
        color: colorBg,
        padding: const EdgeInsets.fromLTRB(14, 8, 14, 8),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                controller: _composerController,
                onChanged: (_) => setState(() {}),
                onSubmitted: (_) => _sendMessage(),
                textInputAction: TextInputAction.send,
                style: const TextStyle(
                  fontFamily: 'Helvetica',
                  fontSize: 14,
                  color: colorFg,
                  height: 1.3,
                ),
                decoration: const InputDecoration(
                  border: InputBorder.none,
                  hintText: 'MESSAGE THE GROUP…',
                  hintStyle: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    color: colorFg4,
                  ),
                  contentPadding: EdgeInsets.symmetric(
                    horizontal: 4,
                    vertical: 10,
                  ),
                  isDense: true,
                ),
              ),
            ),
            const SizedBox(width: 8),
            Material(
              color: canSend ? colorAccent : colorSurface2,
              child: InkWell(
                onTap: canSend ? _sendMessage : null,
                child: SizedBox(
                  width: 64,
                  height: 40,
                  child: Center(
                    child: Text(
                      'SEND',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.08 * 10,
                        color: canSend ? colorAccentInk : colorFg4,
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

  static const _accentMetaStyle = TextStyle(
    fontFamily: 'JetBrainsMono',
    fontSize: 9,
    fontWeight: FontWeight.w700,
    letterSpacing: 0.08 * 9,
    color: colorAccent,
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

class _StackedAvatar extends StatelessWidget {
  final GroupMemberDto member;
  final bool isMe;
  final int overflow;

  const _StackedAvatar({
    required this.member,
    required this.isMe,
    required this.overflow,
  });

  @override
  Widget build(BuildContext context) {
    final live = member.stageId != null;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Container(
          width: 44,
          height: 44,
          decoration: BoxDecoration(
            color: isMe ? colorAccent : colorSurface2,
            border: Border.all(color: colorSurface1, width: 2),
          ),
          alignment: Alignment.center,
          child: Text(
            overflow > 0 ? '+$overflow' : _avatarInitials(member.displayName),
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 11,
              fontWeight: FontWeight.w700,
              color: isMe ? colorAccentInk : colorFg,
            ),
          ),
        ),
        if (live && overflow == 0)
          Positioned(
            right: -2,
            bottom: -2,
            child: Container(
              width: 10,
              height: 10,
              decoration: BoxDecoration(
                color: colorCoAccent,
                shape: BoxShape.circle,
                border: Border.all(color: colorSurface1, width: 2),
              ),
            ),
          ),
      ],
    );
  }
}

class _GroupActionsSheet extends StatelessWidget {
  final List<GroupInfo> groups;
  final String? activeGroupId;
  final ValueChanged<String> onSwitch;
  final VoidCallback onCreate;
  final VoidCallback onJoin;
  final VoidCallback? onLeave;

  const _GroupActionsSheet({
    required this.groups,
    required this.activeGroupId,
    required this.onSwitch,
    required this.onCreate,
    required this.onJoin,
    this.onLeave,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      color: colorSurface1,
      child: SafeArea(
        top: false,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 36,
              height: 3,
              margin: const EdgeInsets.only(top: 8),
              color: colorFg4,
            ),
            DottedBorder.bottom(
              child: const SizedBox(
                height: 58,
                child: Padding(
                  padding: EdgeInsets.symmetric(horizontal: 18),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          'GROUPS',
                          style: TextStyle(
                            fontFamily: 'Helvetica',
                            fontSize: 22,
                            fontWeight: FontWeight.w700,
                            color: colorFg,
                          ),
                        ),
                      ),
                      Text('MANAGE //', style: _sheetMetaStyle),
                    ],
                  ),
                ),
              ),
            ),
            for (final group in groups)
              _GroupActionRow(
                icon: group.id == activeGroupId
                    ? Icons.radio_button_checked
                    : Icons.radio_button_off,
                label: group.name.toUpperCase(),
                meta: group.id == activeGroupId ? 'ACTIVE' : 'SWITCH',
                accent: group.id == activeGroupId,
                onTap: group.id == activeGroupId
                    ? null
                    : () => onSwitch(group.id),
              ),
            _GroupActionRow(
              icon: Icons.qr_code_scanner,
              label: 'JOIN GROUP',
              meta: 'CODE OR QR',
              onTap: onJoin,
            ),
            _GroupActionRow(
              icon: Icons.add,
              label: 'CREATE GROUP',
              meta: 'NEW CREW',
              onTap: onCreate,
            ),
            if (onLeave != null)
              _GroupActionRow(
                icon: Icons.logout,
                label: 'LEAVE ACTIVE GROUP',
                meta: 'LOCAL LIKES KEPT',
                destructive: true,
                onTap: onLeave,
              ),
          ],
        ),
      ),
    );
  }
}

class _GroupActionRow extends StatelessWidget {
  final IconData icon;
  final String label;
  final String meta;
  final bool accent;
  final bool destructive;
  final VoidCallback? onTap;

  const _GroupActionRow({
    required this.icon,
    required this.label,
    required this.meta,
    this.accent = false,
    this.destructive = false,
    this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    final color = destructive ? colorErr : (accent ? colorAccent : colorFg);
    return DottedBorder.bottom(
      child: Material(
        color: Colors.transparent,
        child: InkWell(
          onTap: onTap,
          child: SizedBox(
            height: 54,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 18),
              child: Row(
                children: [
                  Icon(icon, size: 17, color: color),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Text(
                      label,
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        color: color,
                      ),
                    ),
                  ),
                  Text(meta, style: _sheetMetaStyle),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

String _avatarInitials(String name) {
  final parts = name.trim().split(RegExp(r'\s+'));
  if (parts.length >= 2) return '${parts.first[0]}${parts[1][0]}'.toUpperCase();
  return name.substring(0, name.length.clamp(0, 2)).toUpperCase();
}

const _sheetMetaStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.06 * 9,
  color: colorFg3,
);

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
