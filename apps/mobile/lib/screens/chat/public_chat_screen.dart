import 'dart:async';

import 'package:flutter/material.dart';

import '../../data/check_in_controller.dart';
import '../../src/rust/api.dart';
import '../../src/rust/api/dto.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class PublicChatScreen extends StatefulWidget {
  final AppNode node;
  final String festivalId;
  final String channelId;
  final String title;
  final String userId;
  final CheckInController? checkInController;

  const PublicChatScreen({
    super.key,
    required this.node,
    required this.festivalId,
    required this.channelId,
    required this.title,
    required this.userId,
    this.checkInController,
  });

  @override
  State<PublicChatScreen> createState() => _PublicChatScreenState();
}

class _PublicChatScreenState extends State<PublicChatScreen> {
  final _composer = TextEditingController();
  final _scroll = ScrollController();
  StreamSubscription<List<ChatMessageDto>>? _subscription;
  List<ChatMessageDto> _messages = const [];
  int _loaded = 50;
  bool _loading = true;
  bool _sending = false;
  String? _error;

  bool get _campsite => widget.channelId == 'campsite';
  String get _topic => 'festival/${widget.festivalId}/chat/${widget.channelId}';

  @override
  void initState() {
    super.initState();
    unawaited(_start());
  }

  @override
  void dispose() {
    _subscription?.cancel();
    _composer.dispose();
    _scroll.dispose();
    super.dispose();
  }

  Future<void> _start() async {
    try {
      await widget.node.subscribeChatTopics(
        festivalId: widget.festivalId,
        stageIds: _campsite ? const [] : [widget.channelId],
      );
      final stream = await widget.node.watchChat(topic: _topic, lastN: 50);
      _subscription = stream.listen((messages) {
        if (!mounted) return;
        if (_loaded > 50) {
          unawaited(_reload());
          return;
        }
        setState(() {
          _messages = messages;
          _loading = false;
        });
        _scrollToBottom();
      });
      await _reload();
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _loading = false;
        _error = 'CHAT IS UNAVAILABLE ON THIS DEVICE';
      });
    }
  }

  Future<void> _reload() async {
    final messages = await widget.node.getChatHistory(
      topic: _topic,
      limit: _loaded,
      offset: 0,
    );
    if (!mounted) return;
    setState(() {
      _messages = messages;
      _loading = false;
      _error = null;
    });
  }

  Future<void> _loadEarlier() async {
    final before = _messages.length;
    setState(() => _loaded += 50);
    try {
      await _reload();
      if (mounted && _messages.length == before) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('NO EARLIER MESSAGES STORED')),
        );
      }
    } catch (_) {
      if (mounted) setState(() => _loaded -= 50);
    }
  }

  Future<void> _send() async {
    final text = _composer.text.trim();
    if (text.isEmpty || _sending) return;
    setState(() {
      _sending = true;
      _error = null;
    });
    _composer.clear();
    try {
      await widget.node.sendFestivalChat(
        festivalId: widget.festivalId,
        stageId: _campsite ? null : widget.channelId,
        text: text,
      );
      await _reload();
      _scrollToBottom();
    } catch (_) {
      if (mounted) {
        _composer.text = text;
        setState(() => _error = 'COULD NOT SAVE MESSAGE');
      }
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _checkInHere() async {
    final controller = widget.checkInController;
    if (controller == null) return;
    final success = _campsite
        ? await controller.setCampsite()
        : await controller.setStage(widget.channelId);
    if (!success && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('COULD NOT UPDATE CHECK-IN')),
      );
    }
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scroll.hasClients) {
        _scroll.animateTo(
          _scroll.position.maxScrollExtent,
          duration: durationFast,
          curve: curveBrutalist,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: colorBg,
      body: SafeArea(
        child: Column(
          children: [
            ListenableBuilder(
              listenable: widget.checkInController ?? _composer,
              builder: (context, _) {
                final controller = widget.checkInController;
                final checkedIn = _campsite
                    ? controller?.isAtCampsite ?? false
                    : controller?.isAtStage(widget.channelId) ?? false;
                final stale = checkedIn && (controller?.isStale ?? false);
                return _Header(
                  title: widget.title,
                  onBack: () => Navigator.pop(context),
                  actionLabel: stale
                      ? 'STALE · CHECK IN AGAIN'
                      : checkedIn
                      ? '✓ CHECKED IN'
                      : 'CHECK IN AT CAMP',
                  actionIcon: _campsite
                      ? null
                      : checkedIn && !stale
                      ? Icons.location_on
                      : Icons.location_on_outlined,
                  actionSemanticLabel: stale
                      ? 'Check in here again'
                      : checkedIn
                      ? 'Checked in here'
                      : 'Check in here',
                  actionIconColor: stale
                      ? colorWarn
                      : checkedIn
                      ? colorAccent
                      : colorFg2,
                  actionBusy: controller?.saving ?? false,
                  onAction: controller == null ? null : _checkInHere,
                );
              },
            ),
            Expanded(child: _history()),
            if (_error != null)
              Semantics(
                liveRegion: true,
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(sp4, sp2, sp4, 0),
                  child: Text(
                    _error!,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      letterSpacing: trMeta * 9,
                      color: colorWarn,
                    ),
                  ),
                ),
              ),
            _composerBar(),
          ],
        ),
      ),
    );
  }

  Widget _history() {
    if (_loading) {
      return const Center(
        child: CircularProgressIndicator(color: colorAccent, strokeWidth: 1.5),
      );
    }
    if (_messages.isEmpty) {
      return const Center(
        child: Text(
          'NO MESSAGES STORED\nSTART THE CHANNEL OFFLINE OR ONLINE',
          textAlign: TextAlign.center,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: tMeta,
            height: 1.5,
            letterSpacing: trMeta * tMeta,
            color: colorFg3,
          ),
        ),
      );
    }
    return ListView.builder(
      controller: _scroll,
      padding: const EdgeInsets.only(bottom: sp4),
      itemCount: _messages.length + 1,
      itemBuilder: (context, index) => index == 0
          ? TextButton(
              onPressed: _loadEarlier,
              style: TextButton.styleFrom(
                minimumSize: const Size.fromHeight(tapMin),
                foregroundColor: colorFg3,
                shape: const RoundedRectangleBorder(),
              ),
              child: const Text(
                'LOAD EARLIER',
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: tMeta,
                  letterSpacing: trMeta * tMeta,
                ),
              ),
            )
          : _Message(
              message: _messages[index - 1],
              isMe: _messages[index - 1].userId == widget.userId,
            ),
    );
  }

  Widget _composerBar() {
    return DottedBorder.top(
      child: Container(
        color: colorSurface1,
        padding: const EdgeInsets.fromLTRB(sp3, sp2, sp3, sp2),
        child: Row(
          children: [
            Expanded(
              child: TextField(
                controller: _composer,
                minLines: 1,
                maxLines: 4,
                textInputAction: TextInputAction.send,
                onSubmitted: (_) => _send(),
                style: const TextStyle(fontSize: tBody, color: colorFg),
                decoration: const InputDecoration(
                  border: InputBorder.none,
                  hintText: 'message this channel…',
                  hintStyle: TextStyle(color: colorFg4),
                  contentPadding: EdgeInsets.symmetric(horizontal: sp2),
                ),
              ),
            ),
            const SizedBox(width: sp2),
            SizedBox(
              width: 64,
              height: tapMin,
              child: TextButton(
                onPressed: _sending ? null : _send,
                style: TextButton.styleFrom(
                  backgroundColor: _sending ? colorSurface2 : colorAccent,
                  foregroundColor: _sending ? colorFg3 : colorAccentInk,
                  shape: const RoundedRectangleBorder(),
                ),
                child: Text(
                  _sending ? 'WAIT' : 'SEND',
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    fontWeight: FontWeight.w700,
                    letterSpacing: trMeta * 10,
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

class _Header extends StatelessWidget {
  final String title;
  final VoidCallback onBack;
  final String actionLabel;
  final IconData? actionIcon;
  final String? actionSemanticLabel;
  final Color? actionIconColor;
  final bool actionBusy;
  final VoidCallback? onAction;

  const _Header({
    required this.title,
    required this.onBack,
    required this.actionLabel,
    required this.actionBusy,
    this.actionIcon,
    this.actionSemanticLabel,
    this.actionIconColor,
    this.onAction,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: navH,
        child: Row(
          children: [
            SizedBox(
              width: tapMin,
              height: tapMin,
              child: IconButton(
                onPressed: onBack,
                tooltip: 'Back',
                icon: const Icon(Icons.arrow_back, color: colorFg),
              ),
            ),
            const SizedBox(width: sp2),
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    title.toUpperCase(),
                    overflow: TextOverflow.ellipsis,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: tSmall,
                      fontWeight: FontWeight.w700,
                      letterSpacing: trMeta * tSmall,
                      color: colorFg,
                    ),
                  ),
                  const Text(
                    'SIGNED, LOCAL-FIRST',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      letterSpacing: trMeta * 9,
                      color: colorFg3,
                    ),
                  ),
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.only(right: sp2),
              child: actionIcon == null
                  ? TextButton(
                      onPressed: actionBusy ? null : onAction,
                      child: Text(actionBusy ? 'SAVING…' : actionLabel),
                    )
                  : SizedBox(
                      width: tapMin,
                      height: tapMin,
                      child: IconButton(
                        onPressed: actionBusy ? null : onAction,
                        tooltip: actionSemanticLabel,
                        icon: actionBusy
                            ? const SizedBox(
                                width: 16,
                                height: 16,
                                child: CircularProgressIndicator(
                                  strokeWidth: 1.5,
                                  color: colorFg3,
                                ),
                              )
                            : Icon(actionIcon, color: actionIconColor),
                      ),
                    ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Message extends StatelessWidget {
  final ChatMessageDto message;
  final bool isMe;

  const _Message({required this.message, required this.isMe});

  @override
  Widget build(BuildContext context) {
    final parsed = DateTime.tryParse(message.timestamp)?.toLocal();
    final time = parsed == null
        ? ''
        : '${parsed.hour.toString().padLeft(2, '0')}:${parsed.minute.toString().padLeft(2, '0')}';
    final (trustLabel, trustColor) = switch (message.trust) {
      'verified' => ('VERIFIED', colorCoAccent),
      'verifiedOffline' => ('VERIFIED OFFLINE', colorCoAccent),
      _ => ('UNVERIFIED', colorWarn),
    };

    return Padding(
      padding: const EdgeInsets.fromLTRB(sp4, sp2, sp4, sp2),
      child: Align(
        alignment: isMe ? Alignment.centerRight : Alignment.centerLeft,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 360),
          child: DottedBorder(
            color: isMe ? colorAccent.withValues(alpha: 0.65) : colorDotted,
            child: Container(
              color: isMe ? colorAccentDim : colorSurface1,
              padding: const EdgeInsets.all(sp3),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Flexible(
                        child: Text(
                          isMe ? 'YOU' : message.displayName.toUpperCase(),
                          overflow: TextOverflow.ellipsis,
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            fontWeight: FontWeight.w700,
                            color: colorFg,
                          ),
                        ),
                      ),
                      const SizedBox(width: sp2),
                      Text(
                        time,
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          color: colorFg4,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: sp1),
                  Text(
                    message.text,
                    style: const TextStyle(
                      fontSize: tBody,
                      height: 1.35,
                      color: colorFg,
                    ),
                  ),
                  const SizedBox(height: sp2),
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.verified_user_outlined,
                        size: 12,
                        color: trustColor,
                      ),
                      const SizedBox(width: sp1),
                      Text(
                        trustLabel,
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 8,
                          fontWeight: FontWeight.w700,
                          letterSpacing: trMeta * 8,
                          color: trustColor,
                        ),
                      ),
                    ],
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
