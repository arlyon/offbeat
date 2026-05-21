// OFFBEAT TopNav — 52px
// Dotted bottom border
// Left: Mark + "OFFBEAT//" wordmark (mono 12px, "//" in accent)
// Right: icon buttons (WifiOff, Settings) or custom right widgets
// Optional back chevron mode

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import '../widgets/mark.dart';
import '../widgets/dotted_border.dart';

class TopNav extends StatelessWidget {
  final String? festivalName;
  final List<Widget> rightWidgets;
  final bool showBack;
  final VoidCallback? onBack;

  const TopNav({
    super.key,
    this.festivalName,
    this.rightWidgets = const [],
    this.showBack = false,
    this.onBack,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.bottom(
      child: SizedBox(
        height: navH,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              // Left side: back or mark + wordmark
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (showBack)
                    GestureDetector(
                      onTap: onBack,
                      child: SizedBox(
                        width: 36,
                        height: 36,
                        child: Center(
                          child: Icon(Icons.chevron_left, color: colorFg, size: 18),
                        ),
                      ),
                    )
                  else
                    const OffbeatMark(),
                  const SizedBox(width: 8),
                  // Wordmark
                  RichText(
                    text: const TextSpan(
                      style: TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 12,
                        fontWeight: FontWeight.w700,
                        letterSpacing: 0.04 * 12,
                        color: colorFg,
                      ),
                      children: [
                        TextSpan(text: 'OFFBEAT'),
                        TextSpan(
                          text: '//',
                          style: TextStyle(color: colorAccent),
                        ),
                      ],
                    ),
                  ),
                  // Optional festival name
                  if (festivalName != null) ...[
                    const SizedBox(width: 4),
                    Container(
                      width: 1,
                      height: 14,
                      color: colorHairline,
                      margin: const EdgeInsets.symmetric(horizontal: 4),
                    ),
                    Flexible(
                      child: Text(
                        festivalName!.toUpperCase(),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: const TextStyle(
                          fontFamily: 'JetBrainsMono',
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                          letterSpacing: 0.04 * 12,
                          color: colorFg2,
                        ),
                      ),
                    ),
                  ],
                ],
              ),
              // Right side
              Row(
                mainAxisSize: MainAxisSize.min,
                children: rightWidgets,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

// Standard icon button for nav
class NavIconButton extends StatelessWidget {
  final IconData icon;
  final VoidCallback? onTap;
  final int? badgeCount;
  final Color? color;

  const NavIconButton({
    super.key,
    required this.icon,
    this.onTap,
    this.badgeCount,
    this.color,
  });

  @override
  Widget build(BuildContext context) {
    return GestureDetector(
      onTap: onTap,
      child: SizedBox(
        width: 36,
        height: 36,
        child: Stack(
          alignment: Alignment.center,
          children: [
            Icon(icon, size: 17, color: color ?? colorFg),
            if (badgeCount != null && badgeCount! > 0)
              Positioned(
                top: 4,
                right: 4,
                child: Container(
                  width: 14,
                  height: 14,
                  color: colorAccent,
                  child: Center(
                    child: Text(
                      '$badgeCount',
                      style: const TextStyle(
                        fontFamily: 'JetBrainsMono',
                        fontSize: 9,
                        fontWeight: FontWeight.w700,
                        color: colorAccentInk,
                        height: 1,
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
