import 'dart:developer' as dev;
import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../shell/top_nav.dart';
import '../../widgets/dotted_border.dart';

/// Shown when the user has not yet registered a passkey.
/// Single call-to-action: "Set up passkey" button.
class RegistrationScreen extends StatefulWidget {
  final Future<void> Function() onRegister;

  const RegistrationScreen({super.key, required this.onRegister});

  @override
  State<RegistrationScreen> createState() => _RegistrationScreenState();
}

class _RegistrationScreenState extends State<RegistrationScreen> {
  bool _loading = false;
  String? _error;

  Future<void> _handleRegister() async {
    setState(() {
      _loading = true;
      _error = null;
    });
    try {
      await widget.onRegister();
    } catch (e, stack) {
      dev.log('Registration failed', error: e, stackTrace: stack, name: 'auth');
      setState(() => _error = e.toString());
    } finally {
      if (mounted) setState(() => _loading = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        const TopNav(),
        Expanded(
          child: Padding(
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
                    child: Icon(
                      Icons.fingerprint,
                      color: colorAccent,
                      size: 28,
                    ),
                  ),
                ),
                const SizedBox(height: 24),
                const Text(
                  'SET UP YOUR IDENTITY',
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
                  'CREATE A PASSKEY TO VERIFY YOUR\nIDENTITY AND START PARTICIPATING',
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
                // Register button
                SizedBox(
                  width: double.infinity,
                  height: 48,
                  child: DottedBorder(
                    child: Material(
                      color: _loading ? colorSurface2 : colorAccent,
                      child: InkWell(
                        onTap: _loading ? null : _handleRegister,
                        child: Center(
                          child: _loading
                              ? const SizedBox(
                                  width: 16,
                                  height: 16,
                                  child: CircularProgressIndicator(
                                    strokeWidth: 1.5,
                                    color: colorFg,
                                  ),
                                )
                              : const Text(
                                  'SET UP PASSKEY',
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
          ),
        ),
      ],
    );
  }
}
