import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../shell/top_nav.dart';
import '../../widgets/dotted_border.dart';

/// Profile screen shown when the user is registered.
/// Displays identity info and auth status.
class YouScreen extends StatelessWidget {
  final String userId;
  final String publicKeyHex;
  final String? displayName;
  final String authState; // "valid", "expiring", "expired"
  final String? expiresAt;
  final ValueChanged<String> onDisplayNameChanged;

  const YouScreen({
    super.key,
    required this.userId,
    required this.publicKeyHex,
    this.displayName,
    required this.authState,
    this.expiresAt,
    required this.onDisplayNameChanged,
  });

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        const TopNav(),
        Expanded(
          child: Padding(
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
                      child:
                          Icon(Icons.fingerprint, color: colorAccent, size: 28),
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
                _InfoRow(label: 'USER ID', value: userId),
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
              ],
            ),
          ),
        ),
      ],
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

class _InfoRow extends StatelessWidget {
  final String label;
  final String value;
  final Color? valueColor;

  const _InfoRow({
    required this.label,
    required this.value,
    this.valueColor,
  });

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
