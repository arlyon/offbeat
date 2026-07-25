import 'package:flutter/material.dart';

import '../../src/rust/api.dart' as rust;
import '../../src/rust/api/dto.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

class MeshtasticDebugSheet extends StatefulWidget {
  final rust.AppNode? node;
  final String? festivalId;

  const MeshtasticDebugSheet({super.key, this.node, this.festivalId});

  @override
  State<MeshtasticDebugSheet> createState() => _MeshtasticDebugSheetState();
}

class _MeshtasticDebugSheetState extends State<MeshtasticDebugSheet> {
  final TextEditingController _bodyController = TextEditingController(
    text: 'offbeat meshtastic debug probe',
  );
  final TextEditingController _groupIdController = TextEditingController();
  final TextEditingController _groupTextController = TextEditingController(
    text: 'mesh hello',
  );

  bool _busy = false;
  String? _error;
  List<MeshtasticDebugDeviceDto> _devices = const [];
  MeshtasticDebugDeviceDto? _selected;
  MeshtasticDebugReportDto? _report;

  @override
  void dispose() {
    _bodyController.dispose();
    _groupIdController.dispose();
    _groupTextController.dispose();
    super.dispose();
  }

  Future<void> _scan() async {
    setState(() {
      _busy = true;
      _error = null;
      _report = null;
    });
    try {
      final devices = await rust.meshtasticDebugScan(scanMs: 5000);
      setState(() {
        _devices = devices;
        _selected = devices.isEmpty ? null : devices.first;
      });
    } catch (error) {
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _probe({required bool send}) async {
    final selected = _selected;
    if (selected == null) return;
    setState(() {
      _busy = true;
      _error = null;
      _report = null;
    });
    try {
      final report = await rust.meshtasticDebugProbe(
        deviceId: selected.deviceId,
        body: _bodyController.text.codeUnits,
        listenMs: send ? 8000 : 30000,
        send: send,
      );
      setState(() => _report = report);
    } catch (error) {
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _sendGroupChat() async {
    final selected = _selected;
    final node = widget.node;
    final groupId = _groupIdController.text.trim();
    if (selected == null || node == null || groupId.isEmpty) return;
    setState(() {
      _busy = true;
      _error = null;
      _report = null;
    });
    try {
      final report = await node.meshtasticSendGroupChat(
        deviceId: selected.deviceId,
        groupId: groupId,
        text: _groupTextController.text,
      );
      setState(() => _report = report);
    } catch (error) {
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _listenApplyGroupChats() async {
    final selected = _selected;
    final node = widget.node;
    final festivalId = widget.festivalId;
    if (selected == null || node == null || festivalId == null) return;
    setState(() {
      _busy = true;
      _error = null;
      _report = null;
    });
    try {
      final report = await node.meshtasticListenApplyGroupChats(
        deviceId: selected.deviceId,
        festivalId: festivalId,
        listenMs: 30000,
      );
      setState(() => _report = report);
    } catch (error) {
      setState(() => _error = error.toString());
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return DraggableScrollableSheet(
      initialChildSize: 0.9,
      minChildSize: 0.55,
      maxChildSize: 0.96,
      expand: false,
      builder: (context, scrollController) => Container(
        color: colorBg,
        padding: const EdgeInsets.fromLTRB(20, 12, 20, 20),
        child: ListView(
          controller: scrollController,
          children: [
            Center(child: Container(width: 32, height: 3, color: colorFg4)),
            const SizedBox(height: 20),
            const Text(
              'MESHTASTIC TEST RIG',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 13,
                fontWeight: FontWeight.w800,
                letterSpacing: 1.2,
                color: colorAccent,
                height: 1,
              ),
            ),
            const SizedBox(height: 10),
            const Text(
              'Scans official Meshtastic BLE, subscribes to FromNum, drains FromRadio, and decodes Offbeat PRIVATE_APP frames.',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 10,
                color: colorFg3,
                height: 1.4,
              ),
            ),
            const SizedBox(height: 20),
            _ActionButton(
              label: _busy ? 'WORKING...' : 'SCAN RADIOS',
              onTap: _busy ? null : _scan,
            ),
            const SizedBox(height: 16),
            if (_devices.isNotEmpty) ...[
              const _SectionLabel('RADIOS'),
              const SizedBox(height: 8),
              ..._devices.map(_deviceRow),
              const SizedBox(height: 20),
              const _SectionLabel('PAYLOAD'),
              const SizedBox(height: 8),
              DottedBorder(
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  child: TextField(
                    controller: _bodyController,
                    maxLines: 2,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      color: colorFg,
                    ),
                    decoration: const InputDecoration(
                      border: InputBorder.none,
                      hintText: 'debug payload',
                      hintStyle: TextStyle(color: colorFg4),
                    ),
                  ),
                ),
              ),
              const SizedBox(height: 12),
              Row(
                children: [
                  Expanded(
                    child: _ActionButton(
                      label: 'LISTEN 30S',
                      onTap: _busy ? null : () => _probe(send: false),
                    ),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: _ActionButton(
                      label: 'SEND + LISTEN',
                      onTap: _busy ? null : () => _probe(send: true),
                    ),
                  ),
                ],
              ),
              if (widget.node != null) ...[
                const SizedBox(height: 24),
                const _SectionLabel('DOMAIN E2E: GROUP CHAT'),
                const SizedBox(height: 8),
                const Text(
                  'Use the group ID from Social. Receiver must be inside the same festival so it can match the hidden group topic tag.',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    color: colorFg4,
                    height: 1.4,
                  ),
                ),
                const SizedBox(height: 8),
                DottedBorder(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    child: TextField(
                      controller: _groupIdController,
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 11,
                        color: colorFg,
                      ),
                      decoration: const InputDecoration(
                        border: InputBorder.none,
                        hintText: 'group id',
                        hintStyle: TextStyle(color: colorFg4),
                      ),
                    ),
                  ),
                ),
                const SizedBox(height: 8),
                DottedBorder(
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    child: TextField(
                      controller: _groupTextController,
                      maxLength: 96,
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 11,
                        color: colorFg,
                      ),
                      decoration: const InputDecoration(
                        border: InputBorder.none,
                        counterStyle: TextStyle(color: colorFg4),
                        hintText: 'message',
                        hintStyle: TextStyle(color: colorFg4),
                      ),
                    ),
                  ),
                ),
                const SizedBox(height: 12),
                Row(
                  children: [
                    Expanded(
                      child: _ActionButton(
                        label: 'LISTEN + APPLY',
                        onTap: _busy || widget.festivalId == null
                            ? null
                            : _listenApplyGroupChats,
                      ),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: _ActionButton(
                        label: 'SEND GROUP CHAT',
                        onTap: _busy ? null : _sendGroupChat,
                      ),
                    ),
                  ],
                ),
              ],
            ],
            if (_error != null) ...[
              const SizedBox(height: 20),
              _LogBlock(title: 'ERROR', lines: [_error!], color: colorErr),
            ],
            if (_report != null) ...[
              const SizedBox(height: 20),
              _ReportView(report: _report!),
            ],
          ],
        ),
      ),
    );
  }

  Widget _deviceRow(MeshtasticDebugDeviceDto device) {
    final selected = device.deviceId == _selected?.deviceId;
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: DottedBorder(
        color: selected ? colorAccent : colorDotted,
        child: Material(
          color: selected ? colorSurface2 : Colors.transparent,
          child: InkWell(
            onTap: () => setState(() => _selected = device),
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Row(
                children: [
                  Icon(
                    selected
                        ? Icons.radio_button_checked
                        : Icons.radio_button_off,
                    color: selected ? colorAccent : colorFg4,
                    size: 16,
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          device.name ?? 'Meshtastic radio',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 11,
                            color: colorFg,
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        const SizedBox(height: 4),
                        Text(
                          '${device.deviceId}  RSSI ${device.rssi ?? 0}',
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 9,
                            color: colorFg4,
                          ),
                        ),
                      ],
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

class _ReportView extends StatelessWidget {
  final MeshtasticDebugReportDto report;

  const _ReportView({required this.report});

  @override
  Widget build(BuildContext context) {
    final summary = [
      'device=${report.deviceId}',
      'mtu=${report.mtu}',
      'sent_fragments=${report.sentFragments}',
      'raw_from_radio=${report.rawFromRadioCount}',
      'private_app=${report.privateAppCount}',
      'applied_group_chats=${report.appliedGroupChats}',
      'decoded_frames=${report.receivedFrames.length}',
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _LogBlock(title: 'REPORT', lines: summary, color: colorOk),
        const SizedBox(height: 12),
        _LogBlock(title: 'EVENTS', lines: report.events, color: colorFg2),
        if (report.receivedFrames.isNotEmpty) ...[
          const SizedBox(height: 12),
          _LogBlock(
            title: 'DECODED FRAMES',
            lines: report.receivedFrames
                .map(
                  (frame) =>
                      '${frame.kind} ${frame.messageIdHex}: ${frame.bodyText ?? frame.bodyHex}',
                )
                .toList(),
            color: colorAccent,
          ),
        ],
      ],
    );
  }
}

class _ActionButton extends StatelessWidget {
  final String label;
  final VoidCallback? onTap;

  const _ActionButton({required this.label, this.onTap});

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 44,
      child: DottedBorder(
        color: onTap == null ? colorFg4 : colorAccent,
        child: Material(
          color: onTap == null ? Colors.transparent : colorSurface2,
          child: InkWell(
            onTap: onTap,
            child: Center(
              child: Text(
                label,
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  fontWeight: FontWeight.w800,
                  letterSpacing: 0.8,
                  color: onTap == null ? colorFg4 : colorFg,
                  height: 1,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _LogBlock extends StatelessWidget {
  final String title;
  final List<String> lines;
  final Color color;

  const _LogBlock({
    required this.title,
    required this.lines,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder(
      color: color,
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              title,
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                fontWeight: FontWeight.w800,
                letterSpacing: 0.8,
                color: color,
              ),
            ),
            const SizedBox(height: 8),
            ...lines.map(
              (line) => Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: SelectableText(
                  line,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 9,
                    color: colorFg2,
                    height: 1.35,
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

class _SectionLabel extends StatelessWidget {
  final String label;

  const _SectionLabel(this.label);

  @override
  Widget build(BuildContext context) {
    return Text(
      label,
      style: const TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: 9,
        fontWeight: FontWeight.w800,
        letterSpacing: 0.8,
        color: colorFg3,
        height: 1,
      ),
    );
  }
}
