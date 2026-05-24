// OFFBEAT ConnectionDrawer — Bottom sheet showing transport status
// Channels: WS relay + BLE with peers, bandwidth per channel
// BLE states: BT off → prompt settings, BT on but not started → START, running → ONLINE

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import '../services/bluetooth_service.dart';
import '../src/rust/api.dart';

void showConnectionDrawer(
  BuildContext context, {
  required TransportStatusDto? status,
  SyncStatusDto? syncStatus,
  required VoidCallback onStartBle,
}) {
  showModalBottomSheet(
    context: context,
    backgroundColor: colorBg,
    isScrollControlled: true,
    builder: (_) => DraggableScrollableSheet(
      initialChildSize: 0.45,
      minChildSize: 0.3,
      maxChildSize: 0.7,
      expand: false,
      builder: (context, scrollController) => _ConnectionContent(
        status: status,
        syncStatus: syncStatus,
        scrollController: scrollController,
        onStartBle: onStartBle,
      ),
    ),
  );
}

class _ConnectionContent extends StatefulWidget {
  final TransportStatusDto? status;
  final SyncStatusDto? syncStatus;
  final ScrollController scrollController;
  final VoidCallback onStartBle;

  const _ConnectionContent({
    required this.status,
    this.syncStatus,
    required this.scrollController,
    required this.onStartBle,
  });

  @override
  State<_ConnectionContent> createState() => _ConnectionContentState();
}

class _ConnectionContentState extends State<_ConnectionContent> {
  BluetoothState _btState = BluetoothState.unknown;

  @override
  void initState() {
    super.initState();
    _checkBluetooth();
  }

  Future<void> _checkBluetooth() async {
    final state = await BluetoothService.getState();
    if (mounted) setState(() => _btState = state);
  }

  @override
  Widget build(BuildContext context) {
    final relay = widget.status?.relay;
    final ble = widget.status?.ble;
    final bleActive = ble?.active ?? false;

    return Column(
      children: [
        // Handle bar
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 12),
          child: Container(
            width: 32,
            height: 3,
            decoration: BoxDecoration(
              color: colorFg4,
              borderRadius: BorderRadius.circular(1.5),
            ),
          ),
        ),
        // Title
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: 18),
          child: Align(
            alignment: Alignment.centerLeft,
            child: Text(
              'CONNECTIONS',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: trMeta * 11,
                color: colorFg,
                height: 1,
              ),
            ),
          ),
        ),
        const SizedBox(height: 16),
        // Channels
        Expanded(
          child: ListView(
            controller: widget.scrollController,
            padding: const EdgeInsets.symmetric(horizontal: 18),
            children: [
              _ChannelCard(
                label: 'WEBSOCKET RELAY',
                connected: relay?.connected ?? false,
                authenticated: relay?.authenticated ?? false,
                txBps: relay?.txBytesPerSec.toInt() ?? 0,
                rxBps: relay?.rxBytesPerSec.toInt() ?? 0,
                peerCount: relay?.connected == true ? 1 : 0,
                color: const Color(0xFFC77DFF),
              ),
              const SizedBox(height: 12),
              _buildBleCard(ble, bleActive),
              if (widget.syncStatus != null &&
                  widget.syncStatus!.resources.isNotEmpty) ...[
                const SizedBox(height: 20),
                const Text(
                  'SUBSCRIPTIONS',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: trMeta * 11,
                    color: colorFg,
                    height: 1,
                  ),
                ),
                const SizedBox(height: 10),
                ...widget.syncStatus!.resources.map(
                    (r) => _SubscriptionRow(resource: r)),
              ],
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildBleCard(BleStatusDto? ble, bool bleActive) {
    // BLE transport is running
    if (bleActive) {
      return _ChannelCard(
        label: 'BLUETOOTH LE',
        statusLabel: 'ONLINE',
        statusColor: colorCoAccent,
        connected: true,
        txBps: ble?.txBytesPerSec.toInt() ?? 0,
        rxBps: ble?.rxBytesPerSec.toInt() ?? 0,
        peerCount: ble?.peerCount ?? 0,
        color: colorCoAccent,
        peers: ble?.peers ?? [],
        retransmits: ble?.retransmits.toInt() ?? 0,
      );
    }

    // BT adapter is off
    if (_btState == BluetoothState.off) {
      return _BlePromptCard(
        statusLabel: 'BLUETOOTH OFF',
        statusColor: colorErr,
        message: 'Turn on Bluetooth to sync with nearby peers.',
        actionLabel: 'OPEN SETTINGS',
        onAction: () async {
          await BluetoothService.openSettings();
          Future.delayed(const Duration(seconds: 1), _checkBluetooth);
        },
      );
    }

    // BLE permissions not granted
    if (_btState == BluetoothState.permissionDenied) {
      return _BlePromptCard(
        statusLabel: 'NO PERMISSION',
        statusColor: colorErr,
        message: 'Bluetooth permissions are required to sync with nearby peers.',
        actionLabel: 'GRANT',
        onAction: () async {
          await BluetoothService.requestPermissions();
          await _checkBluetooth();
        },
      );
    }

    // BT is on + permissions granted but transport not started
    return _BlePromptCard(
      statusLabel: 'NOT STARTED',
      statusColor: colorWarn,
      message: 'BLE transport was not running at startup.',
      actionLabel: 'START',
      onAction: () {
        Navigator.of(context).pop();
        widget.onStartBle();
      },
    );
  }
}

class _BlePromptCard extends StatelessWidget {
  final String statusLabel;
  final Color statusColor;
  final String message;
  final String actionLabel;
  final VoidCallback onAction;

  const _BlePromptCard({
    required this.statusLabel,
    required this.statusColor,
    required this.message,
    required this.actionLabel,
    required this.onAction,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        color: colorSurface1,
        border: Border.all(color: colorHairline, width: 1),
      ),
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.circle, size: 7, color: statusColor),
              const SizedBox(width: 8),
              const Expanded(
                child: Text(
                  'BLUETOOTH LE',
                  style: TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: trMeta * 11,
                    color: colorFg,
                    height: 1,
                  ),
                ),
              ),
              Text(
                statusLabel,
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  letterSpacing: trMeta * 9,
                  color: statusColor,
                  height: 1,
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Text(
            message,
            style: const TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 10,
              color: colorFg3,
              height: 1.4,
            ),
          ),
          const SizedBox(height: 12),
          GestureDetector(
            onTap: onAction,
            child: Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                border: Border.all(color: colorCoAccent, width: 1.5),
              ),
              child: Text(
                actionLabel,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 10,
                  fontWeight: FontWeight.w700,
                  letterSpacing: trMeta * 10,
                  color: colorCoAccent,
                  height: 1,
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _ChannelCard extends StatelessWidget {
  final String label;
  final String? statusLabel;
  final Color? statusColor;
  final bool connected;
  final bool authenticated;
  final int txBps;
  final int rxBps;
  final int peerCount;
  final Color color;
  final List<TransportPeerDto> peers;
  final int retransmits;

  const _ChannelCard({
    required this.label,
    this.statusLabel,
    this.statusColor,
    required this.connected,
    this.authenticated = false,
    required this.txBps,
    required this.rxBps,
    required this.peerCount,
    required this.color,
    this.peers = const [],
    this.retransmits = 0,
  });

  @override
  Widget build(BuildContext context) {
    final effectiveStatusLabel =
        statusLabel ?? (connected ? 'CONNECTED' : 'OFFLINE');
    final effectiveStatusColor = statusColor ?? (connected ? color : colorFg4);

    return Container(
      decoration: BoxDecoration(
        color: colorSurface1,
        border: Border.all(color: colorHairline, width: 1),
      ),
      padding: const EdgeInsets.all(14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Header: label + status dot
          Row(
            children: [
              Icon(Icons.circle, size: 7, color: connected ? color : colorFg4),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  label,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 11,
                    fontWeight: FontWeight.w700,
                    letterSpacing: trMeta * 11,
                    color: colorFg,
                    height: 1,
                  ),
                ),
              ),
              Text(
                effectiveStatusLabel,
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  letterSpacing: trMeta * 9,
                  color: effectiveStatusColor,
                  height: 1,
                ),
              ),
            ],
          ),
          if (authenticated) ...[
            const SizedBox(height: 4),
            const Text(
              'AUTHENTICATED',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: trMeta * 9,
                color: colorOk,
                height: 1,
              ),
            ),
          ],
          const SizedBox(height: 12),
          // Stats row: peers + bandwidth
          Row(
            children: [
              _Stat(label: 'PEERS', value: '$peerCount'),
              const SizedBox(width: 20),
              _Stat(label: 'UP', value: _formatBps(txBps)),
              const SizedBox(width: 20),
              _Stat(label: 'DOWN', value: _formatBps(rxBps)),
              if (retransmits > 0) ...[
                const SizedBox(width: 20),
                _Stat(label: 'RETX', value: '$retransmits'),
              ],
            ],
          ),
          // BLE peer list
          if (peers.isNotEmpty) ...[
            const SizedBox(height: 12),
            Container(height: 1, color: colorHairline),
            const SizedBox(height: 10),
            ...peers.map((p) => Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Row(
                    children: [
                      Icon(
                        Icons.circle,
                        size: 5,
                        color:
                            p.phase == 'Connected' ? colorOk : colorWarn,
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          p.deviceId.length > 12
                              ? '${p.deviceId.substring(0, 12)}...'
                              : p.deviceId,
                          style: const TextStyle(
                            fontFamily: 'JetBrainsMono',
                            fontSize: 10,
                            color: colorFg2,
                            height: 1,
                          ),
                        ),
                      ),
                      Text(
                        p.phase.toUpperCase(),
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          letterSpacing: trMeta * 9,
                          color: colorFg3,
                          height: 1,
                        ),
                      ),
                    ],
                  ),
                )),
          ],
        ],
      ),
    );
  }
}

class _SubscriptionRow extends StatelessWidget {
  final ResourceSyncStatusDto resource;

  const _SubscriptionRow({required this.resource});

  Color get _dotColor {
    if (resource.error != null) return colorErr;
    if (resource.syncing) return colorWarn;
    if (resource.lastSynced != null) return colorOk;
    return colorFg4;
  }

  /// Shorten resource IDs for display: "festival/glastonbury-2026/state" → "fest/glastonbury-2026/state"
  String get _displayId {
    var id = resource.id;
    if (id.startsWith('festival/')) {
      id = 'fest/${id.substring('festival/'.length)}';
    } else if (id.startsWith('group/')) {
      // Truncate long group hashes
      final parts = id.split('/');
      if (parts.length >= 2 && parts[1].length > 8) {
        parts[1] = '${parts[1].substring(0, 8)}..';
        id = parts.join('/');
      }
    }
    return id;
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Row(
        children: [
          Icon(Icons.circle, size: 5, color: _dotColor),
          const SizedBox(width: 8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  _displayId,
                  style: const TextStyle(
                    fontFamily: 'JetBrainsMono',
                    fontSize: 10,
                    color: colorFg2,
                    height: 1,
                  ),
                ),
                if (resource.error != null) ...[
                  const SizedBox(height: 3),
                  Text(
                    resource.error!,
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      color: colorErr,
                      height: 1.3,
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ],
            ),
          ),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (resource.syncing)
                const Padding(
                  padding: EdgeInsets.only(right: 8),
                  child: Text(
                    'SYNCING',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      letterSpacing: trMeta * 9,
                      color: colorWarn,
                      height: 1,
                    ),
                  ),
                ),
              Padding(
                  padding: const EdgeInsets.only(right: 8),
                  child: Text(
                    '${resource.peerCount}P',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      color: colorFg3,
                      height: 1,
                    ),
                  ),
                ),
              Text(
                '${resource.messagesReceived}\u2193 ${resource.messagesSent}\u2191',
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  color: colorFg3,
                  height: 1,
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _Stat extends StatelessWidget {
  final String label;
  final String value;

  const _Stat({required this.label, required this.value});

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          label,
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 9,
            letterSpacing: trMeta * 9,
            color: colorFg4,
            height: 1,
          ),
        ),
        const SizedBox(height: 4),
        Text(
          value,
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 13,
            fontWeight: FontWeight.w700,
            color: colorFg,
            height: 1,
          ),
        ),
      ],
    );
  }
}

String _formatBps(int bps) {
  if (bps < 1024) return '${bps}B/s';
  if (bps < 1024 * 1024) return '${(bps / 1024).toStringAsFixed(1)}K/s';
  return '${(bps / (1024 * 1024)).toStringAsFixed(1)}M/s';
}
