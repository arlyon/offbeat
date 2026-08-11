import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:qr_flutter/qr_flutter.dart';
import '../../src/rust/api.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';
import 'meshtastic_debug_sheet.dart';

/// Profile screen shown when the user is registered.
/// Displays identity info and auth status.
class YouScreen extends StatelessWidget {
  final String userId;
  final String publicKeyHex;
  final String? displayName;
  final String authState; // "valid", "expiring", "expired"
  final String? expiresAt;
  final bool isAdmin;
  final String adminRequestStatus; // "", "pending", "already_admin"
  final List<String> adminKeys;
  final ValueChanged<String> onDisplayNameChanged;
  final VoidCallback? onRequestAdmin;
  final Future<void> Function()? onLogout;
  final VoidCallback? onLogoutCompleted;
  final AppNode? node;
  final String? currentFestivalId;

  const YouScreen({
    super.key,
    required this.userId,
    required this.publicKeyHex,
    this.displayName,
    required this.authState,
    this.expiresAt,
    this.isAdmin = false,
    this.adminRequestStatus = '',
    this.adminKeys = const [],
    required this.onDisplayNameChanged,
    this.onRequestAdmin,
    this.onLogout,
    this.onLogoutCompleted,
    this.node,
    this.currentFestivalId,
  });

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 24),
      child: ListView(
        children: [
          const SizedBox(height: 32),
          // Identity header
          Center(
            child: Container(
              width: 48,
              height: 48,
              decoration: BoxDecoration(
                border: Border.all(color: colorAccent, width: 1.5),
              ),
              child: const Center(
                child: Icon(Icons.fingerprint, color: colorAccent, size: 28),
              ),
            ),
          ),
          const SizedBox(height: 16),
          Center(
            child: Text(
              displayName?.toUpperCase() ?? 'ANONYMOUS',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 13,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.1 * 13,
                color: colorFg,
                height: 1,
              ),
            ),
          ),
          const SizedBox(height: 32),
          // Info rows
          GestureDetector(
            onTap: () => _showIdDialog(context),
            child: _InfoRow(label: 'USER ID', value: userId),
          ),
          const SizedBox(height: 12),
          _InfoRow(
            label: 'PUBLIC KEY',
            value: '${publicKeyHex.substring(0, 16)}...',
          ),
          const SizedBox(height: 12),
          _InfoRow(
            label: 'AUTH STATUS',
            value: _authStatusText(),
            valueColor: _authStatusColor(),
          ),
          const SizedBox(height: 32),
          // Display name editor
          DottedBorder(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  const Text(
                    'DISPLAY NAME',
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.08 * 9,
                      color: colorFg3,
                      height: 1,
                    ),
                  ),
                  const SizedBox(height: 8),
                  _DisplayNameField(
                    initial: displayName ?? '',
                    onChanged: onDisplayNameChanged,
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 32),
          // Admin section
          if (isAdmin) ...[
            _InfoRow(label: 'ROLE', value: 'ADMIN', valueColor: colorAccent),
          ] else if (adminRequestStatus == 'pending') ...[
            DottedBorder(
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 10,
                ),
                child: Row(
                  children: [
                    const Icon(Icons.hourglass_top, color: colorWarn, size: 14),
                    const SizedBox(width: 8),
                    const Expanded(
                      child: Text(
                        'ADMIN REQUEST PENDING',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.08 * 9,
                          color: colorWarn,
                          height: 1,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ] else if (onRequestAdmin != null) ...[
            SizedBox(
              width: double.infinity,
              height: 44,
              child: DottedBorder(
                child: Material(
                  color: colorSurface2,
                  child: InkWell(
                    onTap: onRequestAdmin,
                    child: const Center(
                      child: Text(
                        'REQUEST ADMIN ACCESS',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.08 * 9,
                          color: colorFg2,
                          height: 1,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
          if (adminKeys.isNotEmpty) ...[
            const SizedBox(height: 32),
            const Text(
              'ADMINS',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.08 * 9,
                color: colorFg3,
                height: 1,
              ),
            ),
            const SizedBox(height: 8),
            ...adminKeys.map(
              (key) => Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Row(
                  children: [
                    Icon(
                      key == publicKeyHex ? Icons.person : Icons.person_outline,
                      color: key == publicKeyHex ? colorAccent : colorFg4,
                      size: 14,
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(
                        '${key.substring(0, 16)}...',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          color: key == publicKeyHex ? colorFg : colorFg3,
                          height: 1.3,
                        ),
                      ),
                    ),
                    if (key == publicKeyHex)
                      const Text(
                        'YOU',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 8,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.1 * 8,
                          color: colorAccent,
                          height: 1,
                        ),
                      ),
                  ],
                ),
              ),
            ),
          ],
          const SizedBox(height: 32),
          SizedBox(
            width: double.infinity,
            height: 44,
            child: DottedBorder(
              color: colorAccent,
              child: Material(
                color: colorSurface2,
                child: InkWell(
                  onTap: () => _showMeshtasticDebug(context),
                  child: const Center(
                    child: Text(
                      'MESHTASTIC TEST RIG',
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 9,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.08 * 9,
                        color: colorFg,
                        height: 1,
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ),
          // Logout button
          if (onLogout != null) ...[
            const SizedBox(height: 48),
            SizedBox(
              width: double.infinity,
              height: 44,
              child: DottedBorder(
                color: colorErr,
                child: Material(
                  color: Colors.transparent,
                  child: InkWell(
                    onTap: () => _showLogoutConfirmation(context),
                    child: const Center(
                      child: Text(
                        'LOG OUT',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.08 * 9,
                          color: colorErr,
                          height: 1,
                        ),
                      ),
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

  void _showIdDialog(BuildContext context) {
    showDialog(
      context: context,
      builder: (_) => _IdQrDialog(userId: userId, publicKeyHex: publicKeyHex),
    );
  }

  Future<void> _showLogoutConfirmation(BuildContext context) async {
    final logout = onLogout;
    if (logout == null) return;
    final completed = await showModalBottomSheet<bool>(
      context: context,
      backgroundColor: colorBg,
      isScrollControlled: true,
      useSafeArea: true,
      builder: (_) => _LogoutConfirmationSheet(onConfirm: logout),
    );
    if (completed == true) onLogoutCompleted?.call();
  }

  void _showMeshtasticDebug(BuildContext context) {
    showModalBottomSheet(
      context: context,
      backgroundColor: colorBg,
      isScrollControlled: true,
      builder: (_) =>
          MeshtasticDebugSheet(node: node, festivalId: currentFestivalId),
    );
  }

  String _authStatusText() {
    switch (authState) {
      case 'valid':
        return 'VERIFIED';
      case 'expiring':
        return 'EXPIRES IN $expiresAt';
      case 'expired':
        return 'EXPIRED';
      default:
        return 'UNKNOWN';
    }
  }

  Color _authStatusColor() {
    switch (authState) {
      case 'valid':
        return colorOk;
      case 'expiring':
        return colorWarn;
      case 'expired':
        return colorErr;
      default:
        return colorFg3;
    }
  }
}

class _LogoutConfirmationSheet extends StatefulWidget {
  final Future<void> Function() onConfirm;

  const _LogoutConfirmationSheet({required this.onConfirm});

  @override
  State<_LogoutConfirmationSheet> createState() =>
      _LogoutConfirmationSheetState();
}

class _LogoutConfirmationSheetState extends State<_LogoutConfirmationSheet> {
  bool _busy = false;
  bool _failed = false;

  Future<void> _confirm() async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _failed = false;
    });
    try {
      await widget.onConfirm();
      if (mounted) Navigator.of(context).pop(true);
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _busy = false;
        _failed = true;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 20, 24, 24),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Text(
            'LOG OUT?',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 13,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.1 * 13,
              color: colorFg,
              height: 1,
            ),
          ),
          const SizedBox(height: 16),
          const _LogoutSummary(
            label: 'REMOVED FROM THIS DEVICE',
            text:
                'Account credentials, personal schedule, check-ins, groups, and private chat.',
            color: colorErr,
          ),
          const SizedBox(height: 14),
          const _LogoutSummary(
            label: 'KEPT OFFLINE',
            text:
                'Downloaded festivals, signed lineups, announcements, public chat, and your OS passkey.',
            color: colorOk,
          ),
          const SizedBox(height: 12),
          const Text(
            'YOU CAN USE THIS PASSKEY TO LOG BACK IN WITHOUT INTERNET.',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 8,
              letterSpacing: 0.06 * 8,
              color: colorFg3,
              height: 1.4,
            ),
          ),
          if (_failed) ...[
            const SizedBox(height: 14),
            const Text(
              "COULDN'T LOG OUT. YOUR ACCOUNT IS STILL ON THIS DEVICE.",
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                fontWeight: FontWeight.w700,
                color: colorErr,
                height: 1.4,
              ),
            ),
          ],
          const SizedBox(height: 24),
          Row(
            children: [
              Expanded(
                child: SizedBox(
                  height: 48,
                  child: DottedBorder(
                    child: Material(
                      color: colorSurface2,
                      child: InkWell(
                        onTap: _busy
                            ? null
                            : () => Navigator.of(context).pop(false),
                        child: const Center(
                          child: Text('CANCEL', style: _logoutButtonStyle),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: SizedBox(
                  height: 48,
                  child: DottedBorder(
                    color: colorErr,
                    child: Material(
                      color: _busy ? colorSurface2 : colorErr,
                      child: InkWell(
                        onTap: _busy ? null : _confirm,
                        child: Center(
                          child: _busy
                              ? const SizedBox(
                                  width: 16,
                                  height: 16,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 1.5,
                                    color: colorFg,
                                  ),
                                )
                              : const Text(
                                  'LOG OUT',
                                  style: TextStyle(
                                    fontFamily: 'JetBrainsMono',
                                    fontSize: 9,
                                    fontWeight: FontWeight.w700,
                                    letterSpacing: 0.08 * 9,
                                    color: colorBg,
                                    height: 1,
                                  ),
                                ),
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _LogoutSummary extends StatelessWidget {
  final String label;
  final String text;
  final Color color;

  const _LogoutSummary({
    required this.label,
    required this.text,
    required this.color,
  });

  @override
  Widget build(BuildContext context) {
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Icon(Icons.square, size: 7, color: color),
        ),
        const SizedBox(width: 10),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                label,
                style: TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 8,
                  fontWeight: FontWeight.w700,
                  letterSpacing: 0.08 * 8,
                  color: color,
                  height: 1,
                ),
              ),
              const SizedBox(height: 5),
              Text(
                text,
                style: const TextStyle(
                  fontFamily: 'JetBrainsMono',
                  fontSize: 9,
                  color: colorFg2,
                  height: 1.45,
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

const _logoutButtonStyle = TextStyle(
  fontFamily: 'JetBrainsMono',
  fontSize: 9,
  fontWeight: FontWeight.w700,
  letterSpacing: 0.08 * 9,
  color: colorFg2,
  height: 1,
);

class _InfoRow extends StatelessWidget {
  final String label;
  final String value;
  final Color? valueColor;

  const _InfoRow({required this.label, required this.value, this.valueColor});

  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceBetween,
      children: [
        Text(
          label,
          style: const TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 9,
            fontWeight: FontWeight.w700,
            letterSpacing: 0.08 * 9,
            color: colorFg4,
            height: 1,
          ),
        ),
        Text(
          value,
          style: TextStyle(
            fontFamily: 'JetBrainsMono',
            fontSize: 9,
            letterSpacing: 0.04 * 9,
            color: valueColor ?? colorFg2,
            height: 1,
          ),
        ),
      ],
    );
  }
}

class _DisplayNameField extends StatefulWidget {
  final String initial;
  final ValueChanged<String> onChanged;

  const _DisplayNameField({required this.initial, required this.onChanged});

  @override
  State<_DisplayNameField> createState() => _DisplayNameFieldState();
}

class _DisplayNameFieldState extends State<_DisplayNameField> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.initial);
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: _controller,
      style: const TextStyle(
        fontFamily: 'JetBrainsMono',
        fontSize: 11,
        color: colorFg,
        height: 1.3,
      ),
      decoration: const InputDecoration(
        isDense: true,
        contentPadding: EdgeInsets.symmetric(vertical: 8),
        border: InputBorder.none,
        hintText: 'Enter your name...',
        hintStyle: TextStyle(
          fontFamily: 'JetBrainsMono',
          fontSize: 11,
          color: colorFg4,
        ),
      ),
      onSubmitted: widget.onChanged,
    );
  }
}

class _IdQrDialog extends StatefulWidget {
  final String userId;
  final String publicKeyHex;
  const _IdQrDialog({required this.userId, required this.publicKeyHex});

  @override
  State<_IdQrDialog> createState() => _IdQrDialogState();
}

class _IdQrDialogState extends State<_IdQrDialog> {
  bool _copied = false;

  void _copy(String value) {
    Clipboard.setData(ClipboardData(text: value));
    setState(() => _copied = true);
    Future.delayed(const Duration(seconds: 2), () {
      if (mounted) setState(() => _copied = false);
    });
  }

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: colorBg,
      shape: const RoundedRectangleBorder(),
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'YOUR IDENTITY',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 11,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.08 * 11,
                color: colorFg,
                height: 1,
              ),
            ),
            const SizedBox(height: 20),
            // QR code
            Container(
              padding: const EdgeInsets.all(12),
              color: Colors.white,
              child: QrImageView(
                data: widget.publicKeyHex,
                version: QrVersions.auto,
                size: 180,
                backgroundColor: Colors.white,
              ),
            ),
            const SizedBox(height: 16),
            // Public key
            const Text(
              'PUBLIC KEY',
              style: TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                fontWeight: FontWeight.w700,
                letterSpacing: 0.08 * 9,
                color: colorFg4,
                height: 1,
              ),
            ),
            const SizedBox(height: 6),
            Text(
              '${widget.publicKeyHex.substring(0, 32)}\n${widget.publicKeyHex.substring(32)}',
              textAlign: TextAlign.center,
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                color: colorFg3,
                height: 1.4,
              ),
            ),
            const SizedBox(height: 20),
            // Copy button
            SizedBox(
              width: double.infinity,
              height: 44,
              child: DottedBorder(
                color: _copied ? colorOk : colorDotted,
                child: Material(
                  color: _copied ? colorSurface2 : colorSurface1,
                  child: InkWell(
                    onTap: () => _copy(widget.publicKeyHex),
                    child: Center(
                      child: Text(
                        _copied ? 'COPIED' : 'COPY PUBLIC KEY',
                        style: TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 9,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.08 * 9,
                          color: _copied ? colorOk : colorFg2,
                          height: 1,
                        ),
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
}
