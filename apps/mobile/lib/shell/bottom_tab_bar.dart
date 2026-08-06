// OFFBEAT BottomTabBar — 56px
// Dotted top border
// 4-column grid: FESTIVALS, SCHEDULE, NOW (with live dot), YOU
// Active: accent color + 1.5px accent line on top
// Icons + labels (mono 9px uppercase)

import 'package:flutter/material.dart';
import '../theme/tokens.dart';
import '../widgets/dotted_border.dart';

enum AppTab { schedule, now, social }

class OffbeatTabBar extends StatelessWidget {
  final AppTab activeTab;
  final ValueChanged<AppTab> onTabChanged;
  final int currentSetCount;

  const OffbeatTabBar({
    super.key,
    required this.activeTab,
    required this.onTabChanged,
    this.currentSetCount = 0,
  });

  @override
  Widget build(BuildContext context) {
    return DottedBorder.top(
      child: SizedBox(
        height: tabH,
        child: Row(
          children: [
            _TabItem(
              tab: AppTab.schedule,
              label: 'SCHEDULE',
              icon: Icons.calendar_today,
              activeTab: activeTab,
              onTap: onTabChanged,
            ),
            _TabItem(
              tab: AppTab.now,
              label: 'NOW',
              icon: Icons.radio,
              activeTab: activeTab,
              onTap: onTabChanged,
              badgeCount: currentSetCount,
            ),
            _TabItem(
              tab: AppTab.social,
              label: 'SOCIAL',
              icon: Icons.people_outline,
              activeTab: activeTab,
              onTap: onTabChanged,
            ),
          ],
        ),
      ),
    );
  }
}

class _TabItem extends StatelessWidget {
  final AppTab tab;
  final String label;
  final IconData icon;
  final AppTab activeTab;
  final ValueChanged<AppTab> onTap;
  final int badgeCount;

  const _TabItem({
    required this.tab,
    required this.label,
    required this.icon,
    required this.activeTab,
    required this.onTap,
    this.badgeCount = 0,
  });

  @override
  Widget build(BuildContext context) {
    final isActive = activeTab == tab;
    final color = isActive ? colorAccent : colorFg3;

    return Expanded(
      child: GestureDetector(
        onTap: () => onTap(tab),
        behavior: HitTestBehavior.opaque,
        child: Stack(
          children: [
            // Active top line indicator
            if (isActive)
              Positioned(
                top: 0,
                left: 0,
                right: 0,
                child: Container(height: 1.5, color: colorAccent),
              ),
            // Tab content
            Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  // Icon with optional live dot
                  Stack(
                    clipBehavior: Clip.none,
                    children: [
                      Icon(icon, size: 18, color: color),
                      if (badgeCount > 0)
                        Positioned(
                          top: -8,
                          right: -11,
                          child: Container(
                            constraints: const BoxConstraints(minWidth: 16),
                            height: 16,
                            padding: const EdgeInsets.symmetric(horizontal: 3),
                            color: colorAccent,
                            alignment: Alignment.center,
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
                    ],
                  ),
                  const SizedBox(height: 3),
                  // Label
                  Text(
                    label,
                    style: TextStyle(
                      fontFamily: 'JetBrainsMono',
                      fontSize: 9,
                      fontWeight: FontWeight.w500,
                      color: color,
                      letterSpacing: 0.1 * 9,
                      height: 1,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
