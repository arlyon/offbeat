// OFFBEAT TopNav — 52px
// Dotted bottom border
// Left: Mark + "OFFBEAT//" wordmark (mono 12px, "//" in accent)
// Right: icon buttons (WifiOff, Settings) or custom right widgets
// Optional back chevron mode with animation support

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import '../widgets/mark.dart';
import '../widgets/dotted_border.dart';

class TopNav extends StatelessWidget {
  final String? festivalName;
  final List<Widget> rightWidgets;
  final bool showBack;
  final VoidCallback? onBack;
  final Animation<double>? animation;

  const TopNav({
    super.key,
    this.festivalName,
    this.rightWidgets = const [],
    this.showBack = false,
    this.onBack,
    this.animation,
  });

  static const _curve = Cubic(0.2, 0.7, 0.2, 1.0);

  @override
  Widget build(BuildContext context) {
    final topPadding = MediaQuery.of(context).padding.top;

    // If we have an animation, build animated version
    if (animation != null) {
      return AnimatedBuilder(
        animation: animation!,
        builder: (context, _) => _buildNav(topPadding, animation!.value),
      );
    }

    // Static version
    return _buildNav(topPadding, showBack ? 1.0 : 0.0);
  }

  Widget _buildNav(double topPadding, double t) {
    // t: 0 = lobby (mark visible), 1 = festival (back visible)
    final curvedT = _curve.transform(t.clamp(0.0, 1.0));

    return DottedBorder.bottom(
      child: Padding(
        padding: EdgeInsets.only(top: topPadding),
        child: SizedBox(
          height: navH,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 14),
            child: Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                // Left side: back/mark + wordmark + festival name
                Expanded(
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      // Mark/Back crossfade with slide
                      SizedBox(
                        width: 36,
                        height: 36,
                        child: Stack(
                          children: [
                            // Mark (fades out, slides left)
                            Transform.translate(
                              offset: Offset(-12 * curvedT, 0),
                              child: Opacity(
                                opacity: 1.0 - curvedT,
                                child: const Center(child: OffbeatMark()),
                              ),
                            ),
                            // Back button (fades in, slides from right)
                            Transform.translate(
                              offset: Offset(12 * (1.0 - curvedT), 0),
                              child: Opacity(
                                opacity: curvedT,
                                child: GestureDetector(
                                  onTap: onBack,
                                  child: const Center(
                                    child: Icon(
                                      Icons.chevron_left,
                                      color: colorFg,
                                      size: 18,
                                    ),
                                  ),
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 8),
                      // Wordmark (always visible)
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
                      // Festival name (fades in, slides from right)
                      if (festivalName != null) ...[
                        const SizedBox(width: 4),
                        // Divider
                        Opacity(
                          opacity: curvedT,
                          child: Container(
                            width: 1,
                            height: 14,
                            color: colorHairline,
                            margin: const EdgeInsets.symmetric(horizontal: 4),
                          ),
                        ),
                        // Name
                        Flexible(
                          child: ClipRect(
                            child: Transform.translate(
                              offset: Offset(20 * (1.0 - curvedT), 0),
                              child: Opacity(
                                opacity: curvedT,
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
                            ),
                          ),
                        ),
                      ],
                    ],
                  ),
                ),
                // Right side
                Row(mainAxisSize: MainAxisSize.min, children: rightWidgets),
              ],
            ),
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
