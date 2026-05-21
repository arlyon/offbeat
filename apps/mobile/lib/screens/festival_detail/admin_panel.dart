import 'package:flutter/material.dart';
import '../../theme/tokens.dart';
import '../../widgets/dotted_border.dart';

/// Admin panel shown as a bottom sheet when the admin icon is tapped.
/// Only visible to users who are in the admin list for this festival.
class AdminPanel extends StatelessWidget {
  final String festivalId;
  final String festivalName;
  final List<String> adminKeys;
  final String userPublicKeyHex;
  final VoidCallback? onRefreshLineup;
  final VoidCallback? onExportSigningKey;
  final ValueChanged<String>? onPromoteAdmin;

  const AdminPanel({
    super.key,
    required this.festivalId,
    required this.festivalName,
    required this.adminKeys,
    required this.userPublicKeyHex,
    this.onRefreshLineup,
    this.onExportSigningKey,
    this.onPromoteAdmin,
  });

  @override
  Widget build(BuildContext context) {
    return Container(
      color: colorSurface1,
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.all(20),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              // Handle
              Center(
                child: Container(
                  width: 32,
                  height: 3,
                  decoration: BoxDecoration(
                    color: colorFg4,
                    borderRadius: BorderRadius.circular(1.5),
                  ),
                ),
              ),
              const SizedBox(height: 20),
              // Title
              Row(
                children: [
                  const Icon(Icons.shield, color: colorAccent, size: 16),
                  const SizedBox(width: 8),
                  Text(
                    'ADMIN // ${festivalName.toUpperCase()}',
                    style: const TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 11,
                      fontWeight: FontWeight.w700,
                      letterSpacing: 0.1 * 11,
                      color: colorFg,
                      height: 1,
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 24),
              // Actions
              _AdminAction(
                icon: Icons.refresh,
                label: 'REFRESH LINEUP',
                sublabel: 'Fetch latest from Clashfinder',
                onTap: onRefreshLineup,
              ),
              const SizedBox(height: 12),
              _AdminAction(
                icon: Icons.key,
                label: 'EXPORT SIGNING KEY',
                sublabel: 'Sign updates offline',
                onTap: onExportSigningKey,
              ),
              const SizedBox(height: 24),
              // Admin list
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
              ...adminKeys.map((key) => Padding(
                    padding: const EdgeInsets.only(bottom: 6),
                    child: Row(
                      children: [
                        Icon(
                          key == userPublicKeyHex
                              ? Icons.person
                              : Icons.person_outline,
                          color: key == userPublicKeyHex
                              ? colorAccent
                              : colorFg4,
                          size: 14,
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            '${key.substring(0, 16)}...',
                            style: TextStyle(
                              fontFamily: 'JetBrainsMono',
                              fontSize: 9,
                              color: key == userPublicKeyHex
                                  ? colorFg
                                  : colorFg3,
                              height: 1.3,
                            ),
                          ),
                        ),
                        if (key == userPublicKeyHex)
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
                  )),
            ],
          ),
        ),
      ),
    );
  }
}

class _AdminAction extends StatelessWidget {
  final IconData icon;
  final String label;
  final String sublabel;
  final VoidCallback? onTap;

  const _AdminAction({
    required this.icon,
    required this.label,
    required this.sublabel,
    this.onTap,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder(
      child: Material(
        color: colorSurface2,
        child: InkWell(
          onTap: onTap,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            child: Row(
              children: [
                Icon(icon, color: colorFg2, size: 16),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        label,
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 10,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.08 * 10,
                          color: colorFg,
                          height: 1,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        sublabel.toUpperCase(),
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 8,
                          letterSpacing: 0.06 * 8,
                          color: colorFg4,
                          height: 1,
                        ),
                      ),
                    ],
                  ),
                ),
                const Icon(Icons.chevron_right, color: colorFg4, size: 16),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Dialog shown when connecting to a festival DO with no admins.
/// Prompts the user to claim the admin role.
class AdminBootstrapDialog extends StatelessWidget {
  final String festivalName;
  final VoidCallback onAccept;
  final VoidCallback onDecline;

  const AdminBootstrapDialog({
    super.key,
    required this.festivalName,
    required this.onAccept,
    required this.onDecline,
  });

  @override
  Widget build(BuildContext context) {
    return Dialog(
      backgroundColor: colorSurface1,
      shape: const RoundedRectangleBorder(),
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Icon(Icons.shield_outlined, color: colorAccent, size: 32),
            const SizedBox(height: 16),
            const Text(
              'BECOME ADMIN?',
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
            Text(
              '${festivalName.toUpperCase()} HAS NO ADMIN YET.\nYOU WILL BE ABLE TO MANAGE LINEUP\nDATA AND PUSH SIGNED UPDATES.',
              style: const TextStyle(
                fontFamily: 'JetBrainsMono',
                fontSize: 9,
                letterSpacing: 0.06 * 9,
                color: colorFg3,
                height: 1.5,
              ),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 24),
            Row(
              children: [
                Expanded(
                  child: DottedBorder(
                    child: Material(
                      color: colorSurface2,
                      child: InkWell(
                        onTap: onDecline,
                        child: const Padding(
                          padding: EdgeInsets.symmetric(vertical: 12),
                          child: Center(
                            child: Text(
                              'NOT NOW',
                              style: TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 9,
                                fontWeight: FontWeight.w700,
                                letterSpacing: 0.08 * 9,
                                color: colorFg3,
                                height: 1,
                              ),
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: DottedBorder(
                    child: Material(
                      color: colorAccent,
                      child: InkWell(
                        onTap: onAccept,
                        child: const Padding(
                          padding: EdgeInsets.symmetric(vertical: 12),
                          child: Center(
                            child: Text(
                              'CLAIM',
                              style: TextStyle(
                                fontFamily: 'JetBrainsMono',
                                fontSize: 9,
                                fontWeight: FontWeight.w700,
                                letterSpacing: 0.08 * 9,
                                color: colorAccentInk,
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
      ),
    );
  }
}
