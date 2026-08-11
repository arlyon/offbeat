import 'dart:developer' as dev;
import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

/// Shown when no identity is active on this device.
class RegistrationScreen extends StatefulWidget {
  final Future<void> Function() onUnlock;
  final Future<void> Function() onRegister;

  const RegistrationScreen({
    super.key,
    required this.onUnlock,
    required this.onRegister,
  });

  @override
  State<RegistrationScreen> createState() => _RegistrationScreenState();
}

class _RegistrationScreenState extends State<RegistrationScreen> {
  String? _activeAction;
  String? _error;

  Future<void> _run(String action, Future<void> Function() callback) async {
    setState(() {
      _activeAction = action;
      _error = null;
    });
    try {
      await callback();
    } catch (_) {
      dev.log('Passkey action failed', name: 'auth');
      if (mounted) {
        setState(
          () => _error = action == 'unlock'
              ? "COULDN'T UNLOCK THIS PASSKEY. TRY AGAIN."
              : "COULDN'T SET UP A PASSKEY. CHECK YOUR CONNECTION AND TRY AGAIN.",
        );
      }
    } finally {
      if (mounted) setState(() => _activeAction = null);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 24),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          // Identity icon
          Container(
            width: 48,
            height: 48,
            decoration: BoxDecoration(
              border: Border.all(color: colorDotted, width: 1.5),
            ),
            child: const Center(
              child: Icon(Icons.fingerprint, color: colorAccent, size: 28),
            ),
          ),
          const SizedBox(height: 24),
          const Text(
            'YOUR IDENTITY',
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
            'USE AN EXISTING PASSKEY OFFLINE,\nOR SET UP A NEW IDENTITY ONLINE',
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
          _AuthActionButton(
            label: 'USE EXISTING PASSKEY',
            loading: _activeAction == 'unlock',
            enabled: _activeAction == null,
            primary: true,
            onTap: () => _run('unlock', widget.onUnlock),
          ),
          const SizedBox(height: 8),
          const Text(
            'WORKS OFFLINE WHEN THE PASSKEY IS ON THIS DEVICE',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 8,
              letterSpacing: 0.06 * 8,
              color: colorFg4,
              height: 1.4,
            ),
            textAlign: TextAlign.center,
          ),
          const SizedBox(height: 20),
          _AuthActionButton(
            label: 'SET UP NEW PASSKEY',
            loading: _activeAction == 'register',
            enabled: _activeAction == null,
            primary: false,
            onTap: () => _run('register', widget.onRegister),
          ),
          const SizedBox(height: 8),
          const Text(
            'INTERNET REQUIRED',
            style: TextStyle(
              fontFamily: 'JetBrainsMono',
              fontSize: 8,
              fontWeight: FontWeight.w700,
              letterSpacing: 0.08 * 8,
              color: colorFg4,
              height: 1,
            ),
          ),
          if (_error != null) ...[
            const SizedBox(height: 16),
            Text(
              _error!,
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                color: colorErr,
                height: 1.3,
              ),
              textAlign: TextAlign.center,
            ),
          ],
        ],
      ),
    );
  }
}

class _AuthActionButton extends StatelessWidget {
  final String label;
  final bool loading;
  final bool enabled;
  final bool primary;
  final VoidCallback onTap;

  const _AuthActionButton({
    required this.label,
    required this.loading,
    required this.enabled,
    required this.primary,
    required this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      width: double.infinity,
      height: 48,
      child: DottedBorder(
        color: primary ? colorAccent : colorDotted,
        child: Material(
          color: primary && enabled ? colorAccent : colorSurface2,
          child: InkWell(
            onTap: enabled ? onTap : null,
            child: Center(
              child: loading
                  ? SizedBox(
                      width: 16,
                      height: 16,
                      child: CircularProgressIndicator(
                        strokeWidth: 1.5,
                        color: primary ? colorAccentInk : colorFg,
                      ),
                    )
                  : Text(
                      label,
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 10,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.1 * 10,
                        color: primary && enabled ? colorAccentInk : colorFg2,
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
